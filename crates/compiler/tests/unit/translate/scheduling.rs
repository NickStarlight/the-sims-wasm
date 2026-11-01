//! Execute generated guest code with synchronous and genuinely suspending hosts.
use crate::{
    guestmem::GuestMem,
    pe::{PeImport, PeInfo, PeSection},
    translate::{
        abi,
        emit_helpers::{i32c, reg, set_reg},
        program::translate_program,
    },
    wasm::{
        builder::{FuncDef, WasmBuilder},
        opcodes::Op,
    },
};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn guest_module() -> (Vec<u8>, usize) {
    let mut image = GuestMem::new();
    let mut entry = vec![0xbb, 0x78, 0x56, 0x34, 0x12]; // EBX must survive nested calls.
    for address in [0x401100u32, 0x401200, 0x401300, 0x401400] {
        let after_call = 0x401000 + entry.len() as u32 + 5;
        entry.push(0xe8);
        entry.extend(address.wrapping_sub(after_call).to_le_bytes());
    }
    entry.extend([0x83, 0xc0, 7, 0xc3]);
    image.write(0x401000, &entry);
    // Nested guest call with a tight backedge and no compatibility API traffic.
    image.write(
        0x401100,
        &[
            0x53, 0xb9, 0, 4, 0, 0, 0x31, 0xc0, 0x40, 0x49, 0x75, 0xfc, 0x5b, 0xc3,
        ],
    );
    // Calls a Wasm-native IAT entry directly, never entering env.dispatch.
    image.write(
        0x401200,
        &[
            0x53, 0xb9, 0, 4, 0, 0, 0xff, 0x15, 0, 0x20, 0x40, 0, 0x49, 0x75, 0xf7, 0x5b, 0xc3,
        ],
    );
    // One guest instruction with a long internal Wasm loop.
    image.write(
        0x401300,
        &[0xb9, 0, 0x10, 0, 0, 0xbf, 0, 0, 2, 0, 0xf3, 0xaa, 0xc3],
    );
    let mut straight = vec![0x40; 700];
    straight.push(0xc3);
    image.write(0x401400, &straight);
    let info = PeInfo {
        name: None,
        is_dll: false,
        exports: Vec::new(),
        entry_va: 0x401000,
        image_base: 0x400000,
        size_of_image: 0x3000,
        size_of_headers: 0,
        reloc_rva: 0,
        reloc_size: 0,
        sections: vec![PeSection {
            name: ".text".into(),
            rva: 0x1000,
            virtual_size: 0x800,
            file_offset: 0,
            size_of_raw_data: 0x800,
            characteristics: 0x60000020,
            va: 0x401000,
        }],
        imports: vec![PeImport {
            dll: "native.dll".into(),
            name: Some("increment".into()),
            ordinal: None,
            hint: None,
            iat_va: 0x402000,
        }],
    };
    let program = translate_program(&image, 0, info.entry_va, &info, &[], &[]).unwrap();
    assert_eq!(program.func_addrs.len(), 5);
    assert!(program.diagnostics.is_empty());
    let api_slot = program.func_addrs.len() + 1;
    (program.wasm, api_slot)
}

fn native_api_module() -> Vec<u8> {
    let mut builder = WasmBuilder::new();
    let signature = builder.add_type(&[], &[]);
    builder.import_memory(1, None);
    for name in abi::REG_GLOBALS {
        builder.add_imported_global(name);
    }
    let mut increment = reg(abi::EAX);
    increment.extend(i32c(1));
    increment.push(Op::I32_ADD);
    let mut body = set_reg(abi::EAX, increment);
    let mut return_address = reg(abi::ESP);
    return_address.extend([Op::I32_LOAD, 2, 0]);
    body.extend(set_reg(abi::ACTUAL_RETURN, return_address));
    let mut stack = reg(abi::ESP);
    stack.extend(i32c(4));
    stack.push(Op::I32_ADD);
    body.extend(set_reg(abi::ESP, stack));
    let index = builder.add_func(FuncDef {
        type_idx: signature,
        locals: vec![],
        body,
    });
    builder.export_func("increment", index);
    builder.build()
}

#[test]
fn checkpoints_suspend_tight_loops_native_calls_rep_and_straight_line_code() {
    let (guest, api_slot) = guest_module();
    let native = native_api_module();
    for module in [&guest, &native] {
        wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::WASM1)
            .validate_all(module)
            .unwrap();
    }
    let input = serde_json::json!({"guest": guest, "native": native, "apiSlot": api_slot,
        "registers": abi::REG_GLOBALS});
    let mut child = Command::new("node")
        .args([
            "--input-type=module",
            "-e",
            include_str!("../../fixtures/scheduling.mjs"),
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("checkpoint execution test requires Node with WebAssembly JSPI");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "checkpoint regression:\n{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
