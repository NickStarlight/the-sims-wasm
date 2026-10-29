//! Assemble discovered guest functions into an executable Wasm module.

mod discovery;
mod dispatch;
#[cfg(test)]
#[path = "../../tests/unit/translate/program/dispatch.rs"]
mod dispatch_tests;
mod flow;
mod function;
#[cfg(test)]
#[path = "../../tests/unit/translate/program/hash.rs"]
mod hash_tests;
mod images;
mod model;
mod roots;
mod segments;

use super::error::EmitError;
use super::{
    abi,
    emit::EmitCtx,
    layout::{locals_list, LOCAL_RUNS},
};
use crate::wasm::{
    builder::{FuncDef, WasmBuilder},
    leb::uleb,
    opcodes::I32,
};
use crate::{guestmem::GuestMem, pe::PeInfo};
use dispatch::{emit_idx_of, emit_run, emit_run_at, tramp_body};
use function::emit_function;
pub use model::{InsnMapEntry, ModuleImage, Program};
use segments::seg_ids;
use std::collections::HashMap;

const TRAMPOLINES: u32 = 16384;
const GUEST_MAX_PAGES: u32 = 65536;

/// Discover all functions and translate to one wasm module.
pub fn translate_program(
    image: &GuestMem,
    base: u32,
    entry: u32,
    info: &PeInfo,
    extra_roots: &[u32],
    modules: &[ModuleImage],
) -> Result<Program, EmitError> {
    let mut diagnostics = Vec::new();
    let mut funcs = discovery::discover(
        image,
        base,
        entry,
        info,
        extra_roots,
        modules,
        &mut diagnostics,
    );
    let mut sorted: Vec<u32> = funcs.keys().copied().collect();
    sorted.sort_unstable();

    let n = sorted.len() as u32;
    let mut b = WasmBuilder::new();
    let void_type = b.add_type(&[], &[]);
    let i32_void = b.add_type(&[I32], &[]);
    for gname in abi::REG_GLOBALS {
        b.add_imported_global(gname);
    }
    let budget = b.add_private_i32_global(super::scheduling::BLOCK_INTERVAL);
    debug_assert_eq!(
        budget,
        u32::from(super::scheduling::CHECKPOINT_BUDGET_GLOBAL)
    );
    let control = b.add_private_i32_global(super::emit_fpu::CONTROL_WORD);
    debug_assert_eq!(control, u32::from(abi::FP_CONTROL));
    b.export_global("fpuControl", control);
    let table_min = 1 + n + ((abi::API_END - abi::API_BASE) >> 2) + TRAMPOLINES;
    b.add_imported_table(table_min);
    b.import_memory(256, Some(GUEST_MAX_PAGES));
    let dispatch_import = b.add_import("env", "dispatch", i32_void);
    let block_trace_import = b.add_import("env", "blockTrace", i32_void);
    let fc = b.imported_func_count(); // 2

    // function index mapping
    let mut func_idx_of: HashMap<u32, i32> = HashMap::new();
    for (i, va) in sorted.iter().enumerate() {
        func_idx_of.insert(*va, i as i32 + fc as i32);
    }
    let idx_of = n as i32 + fc as i32;
    let mut iat_shim: HashMap<u32, i32> = HashMap::new();
    for (i, imp) in info.imports.iter().enumerate() {
        iat_shim.insert(imp.iat_va, i as i32 + sorted.len() as i32 + 1);
    }

    // wasm offsets in the trap map are relative to the body start including the
    // locals declaration
    let locals_decl_len: usize = uleb(LOCAL_RUNS.len() as u32).len()
        + LOCAL_RUNS
            .iter()
            .map(|(count, _ty)| uleb(*count).len() + 1)
            .sum::<usize>();

    let mut blocks_by_func: Vec<(u32, Vec<u32>)> = Vec::new();
    let mut insn_maps: Vec<(u32, Vec<InsnMapEntry>)> = Vec::new();
    for addr in sorted.iter() {
        let (blocks, index) = funcs.get_mut(addr).unwrap();
        let (id_map, keys) = seg_ids(blocks, index);
        blocks_by_func.push((*addr, keys));
        let mut map: Vec<InsnMapEntry> = Vec::new();
        let ctx = EmitCtx {
            id: id_map,
            func_idx_of: &func_idx_of,
            idx_of,
            iat_shim: &iat_shim,
            block_trace: Some(block_trace_import),
        };
        let kernel = super::kernels::recognize(image, base, *addr);
        let body = emit_function(
            blocks,
            index,
            &ctx,
            *addr,
            &mut map,
            &mut diagnostics,
            kernel.as_ref(),
        )
        .map_err(|error| {
            EmitError::new(
                error.guest_va,
                format!("function {addr:#x}: {}", error.message),
            )
        })?;
        for m in map.iter_mut() {
            m.wasm_off += locals_decl_len as u32;
        }
        insn_maps.push((*addr, map));
        let index = b.add_func(FuncDef {
            type_idx: void_type,
            locals: locals_list(),
            body,
        });
        b.name_func(index, format!("guest_{addr:08x}"));
    }
    // The encoder needs only emitted functions. Release discovery buffers
    // before allocating the final code section and module byte buffer.
    drop(funcs);

    emit_idx_of(&mut b, idx_of, n, abi::TAB_ADDR);

    // trampoline pool: tiny wasm funcs forwarding to the JS dispatch import
    for k in 0..TRAMPOLINES {
        b.add_func(FuncDef {
            type_idx: void_type,
            locals: vec![],
            body: tramp_body(k, dispatch_import as i32),
        });
    }
    let tramp_base = 1 + n + ((abi::API_END - abi::API_BASE) >> 2);
    b.add_element_segment(
        tramp_base,
        (0..TRAMPOLINES).map(|k| n + fc + 1 + k).collect(),
    );

    b.add_element_segment(
        1,
        sorted
            .iter()
            .enumerate()
            .map(|(i, _)| i as u32 + fc)
            .collect(),
    );
    b.export_func("idxOf", idx_of as u32);
    let run_idx = emit_run(&mut b, i32_void, idx_of);
    b.export_func("run", run_idx);
    let run_at_idx = emit_run_at(&mut b, i32_void, idx_of);
    b.export_func("runAt", run_at_idx);

    let wasm = b.build();
    Ok(Program {
        diagnostics,
        wasm,
        func_addrs: sorted,
        table_addr: abi::TAB_ADDR,
        table_func_count: n,
        entry,
        tramp_base,
        func_body_starts: b.func_body_starts.clone(),
        blocks_by_func,
        insn_maps,
    })
}
