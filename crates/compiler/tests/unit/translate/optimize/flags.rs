use super::*;
use crate::{
    guestmem::GuestMem,
    translate::{
        abi,
        emit::{emit_insn, EmitCtx},
        emit_helpers::{local_get, local_set},
        test_support::execute_sequences_v8,
    },
    x86::decode::decode_insn,
};
use std::collections::{BTreeMap, HashMap};

fn translate(bytes: &[u8]) -> Vec<u8> {
    let mut memory = GuestMem::new();
    memory.write(0, bytes);
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
    let mut body = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() as u32 {
        let instruction = decode_insn(&memory, 0, offset).unwrap();
        body.extend(emit_insn(&instruction, &context, -1, 2).unwrap());
        offset += instruction.size;
    }
    body
}

#[test]
fn translated_flags_match_independent_native_cpu_results() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("../../../fixtures/native-flags.json")).unwrap();
    let routines: Vec<_> = fixture["routines"]
        .as_array()
        .unwrap()
        .iter()
        .map(|routine| {
            let bytes: Vec<u8> = serde_json::from_value(routine["code"].clone()).unwrap();
            let name: &'static str = Box::leak(
                routine["name"]
                    .as_str()
                    .unwrap()
                    .to_owned()
                    .into_boxed_str(),
            );
            (name, promote(&translate(&bytes)).0)
        })
        .collect();
    let assertions = format!(
        "const fixture = {};\n{}",
        fixture,
        include_str!("../../../fixtures/native-flags.mjs"),
    );
    execute_sequences_v8(&routines, &assertions);
}

/// Private exporter for independent host-CPU checks and paired browser timings.
/// No executable or generated game code is needed by this fixture.
#[test]
#[ignore = "exports experimental Wasm using SIMS_FLAGS_EXPORT_INPUT/OUTPUT"]
fn export_flag_experiments() {
    use crate::{
        translate::{emit_fpu::CONTROL_WORD, layout::locals_list},
        wasm::builder::{FuncDef, WasmBuilder},
    };
    let input = std::env::var("SIMS_FLAGS_EXPORT_INPUT").unwrap();
    let output = std::env::var("SIMS_FLAGS_EXPORT_OUTPUT").unwrap();
    let routines: Vec<(String, Vec<u8>)> =
        serde_json::from_slice(&std::fs::read(input).unwrap()).unwrap();
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(1, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    builder.add_private_i32_global(256);
    builder.add_private_i32_global(CONTROL_WORD);
    for (name, bytes) in routines {
        let body = translate(&bytes);
        let index = builder.add_func(FuncDef {
            type_idx: signature,
            locals: locals_list(),
            body: promote(&body).0,
        });
        builder.export_func(Box::leak(name.into_boxed_str()), index);
    }
    // A bounded arithmetic workload: 20,000 iterations per exported call.
    // Only the final flags in each iteration are architecturally observable.
    let chunk = translate(&[
        0x01, 0xc8, 0x89, 0xc7, 0x31, 0xfb, 0x29, 0xda, 0x40, 0x11, 0xc6, 0xf7, 0xdb, 0xf7, 0xd2,
        0x0f, 0xcb, 0x0f, 0xaf, 0xc1,
    ]);
    let mut body = i32c(20_000);
    body.extend(local_set(0));
    body.extend([Op::LOOP, 0x40]);
    for _ in 0..16 {
        body.extend_from_slice(&chunk);
    }
    body.extend(local_get(0));
    body.extend(i32c(1));
    body.push(Op::I32_SUB);
    body.extend([Op::LOCAL_TEE, 0, Op::BR_IF, 0, Op::END]);
    let index = builder.add_func(FuncDef {
        type_idx: signature,
        locals: locals_list(),
        body: promote(&body).0,
    });
    builder.export_func("benchmark", index);
    let wasm = builder.build();
    wasmparser::Validator::new().validate_all(&wasm).unwrap();
    std::fs::write(output, wasm).unwrap();
}
