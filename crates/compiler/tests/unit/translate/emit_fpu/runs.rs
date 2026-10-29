use super::*;
use crate::{
    guestmem::GuestMem,
    translate::{layout::locals_list, test_support::execute_sequences_v8},
    wasm::builder::{FuncDef, WasmBuilder},
    x86::decode::decode_insn,
};

fn instructions(bytes: &[u8]) -> Vec<Insn> {
    let mut memory = GuestMem::new();
    memory.write(0, bytes);
    let mut result = Vec::new();
    let mut offset = 0;
    while offset < bytes.len() as u32 {
        let insn = decode_insn(&memory, 0, offset).unwrap();
        offset += insn.size;
        result.push(insn);
    }
    result
}

fn sequences(bytes: &[u8]) -> (Seq, Seq) {
    let decoded = instructions(bytes);
    let references: Vec<_> = decoded.iter().collect();
    // Exercise short opcode fixtures too, independently of the production
    // profitability threshold. The reference always uses the original emitter.
    let run = emit_minimum(&references, 4).expect("fixture must use the guarded path");
    assert_eq!(run.count, decoded.len());
    let plain = decoded.iter().flat_map(|i| emit_fpu(i).unwrap()).collect();
    (plain, run.code)
}

#[test]
fn guarded_runs_preserve_stack_bits_aliases_partial_faults_and_registers() {
    let mut cases: Vec<(String, Vec<u8>)> = Vec::new();
    for row in [0xd8, 0xdc, 0xde] {
        for field in [0, 1, 4, 5, 6, 7] {
            for i in [0, 1, 7] {
                let raw = [row, 0xc0 | field << 3 | i];
                cases.push((format!("r_{row:x}_{field}_{i}"), raw.repeat(4)));
            }
        }
    }
    for row in [0xd8, 0xdc] {
        for field in [0, 1, 4, 5, 6, 7] {
            cases.push((format!("m_{row:x}_{field}"), [row, field << 3].repeat(4)));
        }
    }
    for raw in [
        [0xd9, 0xc0],
        [0xd9, 0xc7],
        [0xd9, 0xc8],
        [0xd9, 0xcf],
        [0xdd, 0xd0],
        [0xdd, 0xd7],
        [0xdd, 0xd8],
        [0xdd, 0xdf],
        [0xd9, 0xe0],
        [0xd9, 0xe1],
        [0xd9, 0xfa],
        [0xd9, 0xfc],
        [0xd9, 0xe8],
        [0xd9, 0xe9],
        [0xd9, 0xea],
        [0xd9, 0xeb],
        [0xd9, 0xec],
        [0xd9, 0xed],
        [0xd9, 0xee],
    ] {
        cases.push((format!("op_{:x}_{:x}", raw[0], raw[1]), raw.repeat(4)));
    }
    cases.extend([
        (
            "stack_wrap".into(),
            [[0xd9, 0xe8].repeat(12), [0xdd, 0xd9].repeat(12)].concat(),
        ),
        (
            "single".into(),
            vec![0xd9, 0x00, 0xd8, 0x08, 0xd8, 0x40, 4, 0xd9, 0x19],
        ),
        (
            "double".into(),
            vec![0xdd, 0x00, 0xdc, 0x08, 0xdc, 0x60, 8, 0xdd, 0x19],
        ),
        (
            "store_alias".into(),
            vec![0xdd, 0x00, 0xdd, 0x11, 0xdc, 0x01, 0xdd, 0x19],
        ),
        (
            "single_store".into(),
            vec![0xd9, 0x00, 0xd9, 0x11, 0xd8, 0x01, 0xd9, 0x19],
        ),
        (
            "swap_pop".into(),
            vec![0xdd, 0x00, 0xd9, 0xc1, 0xd9, 0xc9, 0xde, 0xc1, 0xdd, 0x19],
        ),
        (
            "fs_operand".into(),
            vec![0x64, 0xdd, 0x00, 0xd9, 0xe0, 0xd9, 0xe1, 0xdd, 0x19],
        ),
    ]);
    let mut routines = Vec::new();
    for (name, bytes) in &cases {
        let (plain, fast) = sequences(bytes);
        let plain_name: &'static str = Box::leak(format!("{name}_plain").into_boxed_str());
        let fast_name: &'static str = Box::leak(format!("{name}_fast").into_boxed_str());
        routines.push((plain_name, plain));
        routines.push((fast_name, fast));
    }
    execute_sequences_v8(
        &routines,
        include_str!("../../../fixtures/fpu/differential.mjs"),
    );
}

#[test]
fn guarded_runs_stop_before_status_helpers_and_integer_traps() {
    for tail in [
        vec![0xd9, 0xfd], // FSCALE
        vec![0xd9, 0xfe], // FSIN helper
        vec![0xdf, 0xe0], // FNSTSW AX
        vec![0xdb, 0x18], // FISTP m32 (trapping conversion)
        vec![0xdd, 0x30], // FNSAVE
        vec![0xd8, 0xd1], // FCOM ST1
        vec![0x40],       // INC EAX changes memory addressing
    ] {
        let mut bytes = [0xd9, 0xe0].repeat(6);
        bytes.extend(tail);
        bytes.extend([0xd9, 0xe0].repeat(4));
        let decoded = instructions(&bytes);
        assert_eq!(emit(&decoded.iter().collect::<Vec<_>>()).unwrap().count, 6);
    }
}

#[test]
fn compiled_runs_publish_at_checkpoint_and_reload_after_suspension() {
    use crate::{
        pe::{PeInfo, PeSection},
        translate::program::translate_program,
    };
    use std::{
        io::Write,
        process::{Command, Stdio},
    };
    let mut bytes = [0xd9, 0xe8].repeat(254); // FLD1: wrap the physical stack repeatedly
    bytes.extend([0xde, 0xc1, 0xde, 0xc1]); // TOP=4, ST0=3 at the 256-insn checkpoint
    bytes.extend([
        0xd9, 0xe8, 0xde, 0xc1, 0xd9, 0xe0, 0xd9, 0xe1, 0xd9, 0xe0, 0xd9, 0xe1, 0xd9, 0xe0, 0xd9,
        0xe1, 0xc3,
    ]);
    let mut image = GuestMem::new();
    image.write(0x401000, &bytes);
    let info = PeInfo {
        name: None,
        is_dll: false,
        exports: vec![],
        imports: vec![],
        entry_va: 0x401000,
        image_base: 0x400000,
        size_of_image: 0x2000,
        size_of_headers: 0,
        reloc_rva: 0,
        reloc_size: 0,
        sections: vec![PeSection {
            name: ".text".into(),
            rva: 0x1000,
            virtual_size: bytes.len() as u32,
            file_offset: 0,
            size_of_raw_data: bytes.len() as u32,
            characteristics: 0x60000020,
            va: 0x401000,
        }],
    };
    let program = translate_program(&image, 0, info.entry_va, &info, &[], &[]).unwrap();
    assert!(program.diagnostics.is_empty());
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
        .validate_all(&program.wasm)
        .unwrap();
    let input = serde_json::json!({"wasm":program.wasm,"table":program.table_addr,"addresses":program.func_addrs,"names":abi::REG_GLOBALS});
    let mut child = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            include_str!("../../../fixtures/fpu/checkpoint.mjs"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

// Export a small module only when explicitly requested for isolated profiling.
// SIMS_FPU_BENCH_MODULE=/tmp/fpu.wasm cargo test -p sims-compiler --lib export_fpu_benchmark
// The exported module can be used by a local benchmark harness.
#[test]
fn export_fpu_benchmark() {
    let Ok(destination) = std::env::var("SIMS_FPU_BENCH_MODULE") else {
        return;
    };
    // Six load/multiply/add/store instructions, like repeated matrix dot products.
    let bytes = [
        0xdd, 0x00, 0xdc, 0x09, 0xdc, 0x40, 8, 0xdc, 0x49, 8, 0xdc, 0x40, 16, 0xdd, 0x1a,
    ];
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(1, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    let short = vec![0xdd, 0x00, 0xdc, 0x09, 0xdc, 0x40, 8, 0xdd, 0x1a];
    for (name, bytes) in [
        ("short", short),
        ("medium", bytes.to_vec()),
        ("long", bytes.repeat(4)),
    ] {
        let (plain, fast) = sequences(&bytes);
        for (suffix, body) in [("plain", plain), ("fast", fast)] {
            let index = builder.add_func(FuncDef {
                type_idx: signature,
                locals: locals_list(),
                body,
            });
            builder.export_func(
                Box::leak(format!("{name}_{suffix}").into_boxed_str()),
                index,
            );
        }
    }
    let wasm = builder.build();
    wasmparser::Validator::new().validate_all(&wasm).unwrap();
    std::fs::write(destination, wasm).unwrap();
}
