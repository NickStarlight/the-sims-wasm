//! Differential execution of exact native coordinate preparation and its fallback.
use super::*;
use crate::translate::{
    blocks::split_blocks,
    emit_helpers::{i32c, reg, set_reg, Seq},
    kernels::{recognize, Kernel},
    scheduling::CHECKPOINT_BUDGET_GLOBAL,
    test_support::execute_observed_sequences_v8_with_memory,
};
use std::collections::HashMap;

const ADDRESS: u32 = 0x434ffd;
// REA cpu-pass-transform, executable SHA256 6e598c22512a24659cbd0e533306abf3129733d67b1d120916b1b6b1581e29f2.
const TRANSFORM: &str = "8bc18b4c240485c974288a100fbed2c1e1068d5411c00fbe4801c1e20603d1d1e28a8a1ce86d0088088a8a1de86d00884801c20400";

fn image(hex: &str) -> GuestMem {
    let mut image = GuestMem::new();
    let bytes: Vec<_> = (0..hex.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
        .collect();
    image.write(0, &bytes);
    image
}

fn routines() -> Vec<(&'static str, Seq)> {
    let image = image(TRANSFORM);
    assert!(matches!(
        recognize(&image, ADDRESS, ADDRESS),
        Some(Kernel::Geometry)
    ));
    let (blocks, index) = split_blocks(
        &image,
        ADDRESS,
        ADDRESS,
        Some((ADDRESS, ADDRESS + 53)),
        &[(ADDRESS, ADDRESS + 53)],
        &mut Vec::new(),
    )
    .unwrap();
    let (ids, _) = segments::seg_ids(&blocks, &index);
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = EmitCtx {
        memory_minimum_pages: 128,
        id: ids,
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: Some(0),
    };
    let mut routines = Vec::new();
    for (name, fast) in [("transform_plain", false), ("transform_fast", true)] {
        let body = function::emit_function(
            &blocks,
            &index,
            &context,
            ADDRESS,
            &mut Vec::new(),
            &mut Vec::new(),
            fast.then_some(&Kernel::Geometry),
        )
        .unwrap();
        routines.push((name, body));
    }
    let mut changed = image;
    changed.write(0, &[0xff]);
    assert!(recognize(&changed, ADDRESS, ADDRESS).is_none());
    routines.push((
        "set_budget",
        set_reg(CHECKPOINT_BUDGET_GLOBAL, reg(abi::EAX)),
    ));
    routines.push((
        "read_budget",
        set_reg(abi::EAX, reg(CHECKPOINT_BUDGET_GLOBAL)),
    ));
    routines
}

#[test]
fn geometry_preserves_registers_flags_aliases_fault_progress_and_checkpoints() {
    execute_observed_sequences_v8_with_memory(
        &routines(),
        &[0x7f],
        128,
        include_str!("../../../fixtures/geometry.mjs"),
    );
}

#[test]
#[ignore = "exports a standalone geometry benchmark on explicit request"]
fn export_geometry_benchmark() {
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    let observer = builder.add_type(&[0x7f], &[]);
    builder.import_memory(128, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    builder.add_private_i32_global(256);
    builder.add_private_i32_global(crate::translate::emit_fpu::CONTROL_WORD);
    builder.add_import("env", "observe", observer);
    let mut benchmarks = Vec::new();
    for (name, body) in routines() {
        let index = builder.add_func(FuncDef {
            type_idx: signature,
            locals: locals_list(),
            body,
        });
        builder.export_func(name, index);
        if name.contains("_plain") || name.contains("_fast") {
            benchmarks.push((name, index));
        }
    }
    let count_signature = builder.add_type(&[0x7f], &[]);
    for (name, function) in benchmarks {
        let mut body = vec![0x02, 0x40, 0x03, 0x40, 0x20, 0, 0x45, 0x0d, 1];
        body.extend(set_reg(abi::ESP, i32c(0x2000)));
        body.extend(set_reg(abi::ECX, i32c(0x4000)));
        body.extend([0x10]);
        body.extend(uleb(function));
        body.extend([0x20, 0, 0x41, 1, 0x6b, 0x21, 0, 0x0c, 0, 0x0b, 0x0b]);
        let index = builder.add_func(FuncDef {
            type_idx: count_signature,
            locals: Vec::new(),
            body,
        });
        builder.export_func(
            match name {
                "transform_plain" => "bench_transform_plain",
                "transform_fast" => "bench_transform_fast",
                _ => unreachable!(),
            },
            index,
        );
    }
    let module = builder.build();
    wasmparser::Validator::new().validate_all(&module).unwrap();
    std::fs::write(
        std::env::var("SIMS_GEOMETRY_BENCH_MODULE").expect("set benchmark output path"),
        module,
    )
    .unwrap();
}
