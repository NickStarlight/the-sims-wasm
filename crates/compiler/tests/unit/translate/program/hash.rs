//! Compare full native hash functions, including real block checkpoints.
use super::*;
use crate::translate::{
    blocks::split_blocks,
    emit_helpers::{i32c, reg, set_reg, Seq},
    kernels::{recognize, Kernel},
    optimize::promote,
    scheduling::CHECKPOINT_BUDGET_GLOBAL,
    test_support::execute_observed_sequences_v8,
};
use std::collections::HashMap;

const ADDRESS: u32 = 0x5b8dba;
// REA ev_eb585fdb1163f0c2ad55f44a04b83303c50faedd05435ea617328922477628f4.
// Canonical57-byte routine, sufficient to test without an installed game.
const BYTES: &str = "8b54240433c08a0a84c9741c560fbec903c18a4a0169c001040000428bf0c1ee0633c684c975e65e8d0cc08bc1c1e80b33c169c001800000c3";

fn routines() -> Vec<(&'static str, Seq)> {
    let bytes: Vec<_> = (0..BYTES.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&BYTES[i..i + 2], 16).unwrap())
        .collect();
    let mut image = GuestMem::new();
    image.write(0, &bytes);
    assert!(matches!(
        recognize(&image, ADDRESS, ADDRESS),
        Some(Kernel::StringHash)
    ));
    let end = ADDRESS + bytes.len() as u32;
    let (blocks, index) = split_blocks(
        &image,
        ADDRESS,
        ADDRESS,
        Some((ADDRESS, end)),
        &[(ADDRESS, end)],
        &mut Vec::new(),
    )
    .unwrap();
    let (ids, _) = segments::seg_ids(&blocks, &index);
    let functions = HashMap::new();
    let imports = HashMap::new();
    let mut result = Vec::new();
    for (name, fast, polling) in [
        ("plain", false, true),
        ("fast", true, true),
        ("plain_unpolled", false, false),
        ("fast_unpolled", true, false),
    ] {
        let context = EmitCtx {
            id: ids.clone(),
            func_idx_of: &functions,
            idx_of: 0,
            iat_shim: &imports,
            block_trace: polling.then_some(0),
        };
        let body = function::emit_function(
            &blocks,
            &index,
            &context,
            ADDRESS,
            &mut Vec::new(),
            &mut Vec::new(),
            fast.then_some(&Kernel::StringHash),
        )
        .unwrap();
        if polling {
            let mut resumed = set_reg(abi::RESUME_NEXT, i32c(ids[&0x5b8dc7]));
            resumed.extend_from_slice(&body);
            result.push((
                if fast { "fast_resume" } else { "plain_resume" },
                promote(&resumed).0,
            ));
        }
        result.push((name, promote(&body).0));
    }
    result.push((
        "set_budget",
        set_reg(CHECKPOINT_BUDGET_GLOBAL, reg(abi::EAX)),
    ));
    result.push((
        "read_budget",
        set_reg(abi::EAX, reg(CHECKPOINT_BUDGET_GLOBAL)),
    ));
    image.write(15, &[bytes[15] ^ 1]);
    assert!(
        recognize(&image, ADDRESS, ADDRESS).is_none(),
        "whole routine fingerprint"
    );
    result
}

#[test]
fn hash_spans_preserve_native_state_faults_and_checkpoint_reentry() {
    execute_observed_sequences_v8(
        &routines(),
        &[0x7f],
        include_str!("../../../fixtures/hash-spans.mjs"),
    );
}

#[test]
#[ignore = "exports a standalone hash benchmark on explicit request"]
fn export_hash_span_benchmark() {
    let destination = std::env::var("SIMS_HASH_BENCH_MODULE").expect("set benchmark output path");
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    let observer = builder.add_type(&[0x7f], &[]);
    builder.import_memory(1, None);
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
        if matches!(name, "plain" | "fast") {
            benchmarks.push((name, index));
        }
    }
    let count_signature = builder.add_type(&[0x7f], &[]);
    for (name, function) in benchmarks {
        let mut body = vec![0x02, 0x40, 0x03, 0x40, 0x20, 0, 0x45, 0x0d, 1];
        body.extend(set_reg(abi::ESP, i32c(0x2000)));
        body.push(0x10);
        body.extend(uleb(function));
        body.extend([0x20, 0, 0x41, 1, 0x6b, 0x21, 0, 0x0c, 0, 0x0b, 0x0b]);
        let index = builder.add_func(FuncDef {
            type_idx: count_signature,
            locals: Vec::new(),
            body,
        });
        builder.export_func(
            if name == "plain" {
                "bench_plain"
            } else {
                "bench_fast"
            },
            index,
        );
    }
    let module = builder.build();
    wasmparser::Validator::new().validate_all(&module).unwrap();
    std::fs::write(destination, module).unwrap();
}
