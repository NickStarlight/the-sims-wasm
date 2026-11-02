//! Narrow one-operand products must preserve the untouched register halves.
use super::*;
use crate::{pe::PeSection, translate::abi};
use std::{
    io::Write,
    process::{Command, Stdio},
};
#[test]
fn narrow_multiply_preserves_register_halves_and_reports_overflow() {
    const BASE: u32 = 0x401000;
    let mut image = GuestMem::new();
    let mut roots = Vec::new();
    for (index, code) in [
        vec![0xf6, 0xe3, 0xc3],
        vec![0xf6, 0xeb, 0xc3], // MUL/IMUL BL
        vec![0x66, 0xf7, 0xe3, 0xc3],
        vec![0x66, 0xf7, 0xeb, 0xc3], // BX
        vec![0xf6, 0xec, 0xc3],       // IMUL AH aliases the destination
        vec![0x66, 0xf7, 0xe2, 0xc3], // MUL DX aliases the high destination
        vec![0xf6, 0x2e, 0xc3],       // IMUL byte [ESI]
        vec![0x66, 0xf7, 0x26, 0xc3], // MUL word [ESI]
    ]
    .iter()
    .enumerate()
    {
        let address = BASE + index as u32 * 16;
        roots.push(address);
        image.write(address as usize, code);
    }
    let info = crate::pe::PeInfo {
        name: None,
        is_dll: false,
        exports: vec![],
        entry_va: BASE,
        image_base: 0x400000,
        size_of_image: 0x5000,
        size_of_headers: 0,
        reloc_rva: 0,
        reloc_size: 0,
        imports: vec![],
        sections: vec![PeSection {
            name: ".text".into(),
            rva: 0x1000,
            virtual_size: 0x1000,
            file_offset: 0,
            size_of_raw_data: 0x1000,
            characteristics: 0x60000020,
            va: BASE,
        }],
    };
    let program = translate_program(&image, 0, BASE, &info, &roots, &[]).unwrap();
    wasmparser::Validator::new()
        .validate_all(&program.wasm)
        .unwrap();
    let input = serde_json::json!({"wasm":program.wasm,"registers":abi::REG_GLOBALS,"roots":roots});
    let mut child = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/narrow-multiply.mjs"
        ))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(input.to_string().as_bytes())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
