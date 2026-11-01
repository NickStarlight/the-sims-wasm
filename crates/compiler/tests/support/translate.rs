//! Emit small decoded routines and execute them in V8 without a runtime dependency.
use super::{
    abi,
    emit::{emit_insn, EmitCtx},
    layout::locals_list,
};
use crate::{
    guestmem::GuestMem,
    wasm::builder::{FuncDef, WasmBuilder},
    x86::decode::decode_insn,
};
use std::{
    collections::{BTreeMap, HashMap},
    io::Write,
    process::{Command, Stdio},
};

pub(super) fn execute_v8(routines: &[(&'static str, Vec<u8>)], assertions: &str) {
    execute_v8_mode(routines, assertions, false);
}

pub(super) fn execute_optimized_v8(routines: &[(&'static str, Vec<u8>)], assertions: &str) {
    execute_v8_mode(routines, assertions, true);
}

fn execute_v8_mode(routines: &[(&'static str, Vec<u8>)], assertions: &str, optimized: bool) {
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(1, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    builder.add_private_i32_global(256);
    let control = builder.add_private_i32_global(super::emit_fpu::CONTROL_WORD);
    builder.export_global("fpuControl", control);
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = EmitCtx {
        memory_minimum_pages: 1,
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    for (name, code) in routines {
        let mut memory = GuestMem::new();
        memory.write(0, code);
        let mut body = Vec::new();
        let mut offset = 0;
        while offset < code.len() as u32 {
            let instruction = decode_insn(&memory, 0, offset).unwrap();
            let next = offset + instruction.size;
            let shortened = if optimized && next < code.len() as u32 {
                let following = decode_insn(&memory, 0, next).unwrap();
                super::emit::emit_overwritten_flags(&instruction, &following)
            } else {
                None
            };
            body.extend(
                shortened.unwrap_or_else(|| emit_insn(&instruction, &context, -1, 2).unwrap()),
            );
            offset += instruction.size;
        }
        let index = builder.add_func(FuncDef {
            type_idx: signature,
            locals: locals_list(),
            body,
        });
        builder.export_func(name, index);
    }
    let wasm = builder.build();
    check_v8(wasm, assertions);
}

pub(super) fn execute_sequences_v8(routines: &[(&'static str, Vec<u8>)], assertions: &str) {
    execute_observed_sequences_v8(routines, &[], assertions);
}

pub(super) fn execute_observed_sequences_v8(
    routines: &[(&'static str, Vec<u8>)],
    parameters: &[u8],
    assertions: &str,
) {
    execute_observed_sequences_v8_with_memory(routines, parameters, 1, assertions);
}

pub(super) fn execute_observed_sequences_v8_with_memory(
    routines: &[(&'static str, Vec<u8>)],
    parameters: &[u8],
    minimum_pages: u32,
    assertions: &str,
) {
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(minimum_pages, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    builder.add_private_i32_global(256);
    let control = builder.add_private_i32_global(super::emit_fpu::CONTROL_WORD);
    builder.export_global("fpuControl", control);
    let observer = builder.add_type(parameters, &[]);
    builder.add_import("env", "observe", observer);
    for (name, body) in routines {
        let index = builder.add_func(FuncDef {
            type_idx: signature,
            locals: locals_list(),
            body: body.clone(),
        });
        builder.export_func(name, index);
    }
    check_v8(builder.build(), assertions);
}

fn check_v8(wasm: Vec<u8>, assertions: &str) {
    wasmparser::Validator::new().validate_all(&wasm).unwrap();
    let mut child = Command::new("node")
        .args(["--input-type=module", "-e", assertions])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Wasm execution tests require Node.js on PATH (no npm packages)");
    child.stdin.take().unwrap().write_all(&wasm).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "V8 execution regression failed:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
