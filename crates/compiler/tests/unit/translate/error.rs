use crate::guestmem::GuestMem;
use crate::translate::emit::{emit_insn, EmitCtx};
use crate::wasm::opcodes::Op;
use crate::x86::decode::decode_insn;
use crate::x86::types::{Insn, Mnem};
use std::collections::{BTreeMap, HashMap};

fn emit(bytes: &[u8]) -> Result<Vec<u8>, super::EmitError> {
    let mut memory = GuestMem::new();
    memory.write(0, bytes);
    let instruction = decode_insn(&memory, 0x401000, 0x401000).unwrap();
    emit_decoded(&instruction)
}

fn emit_decoded(instruction: &Insn) -> Result<Vec<u8>, super::EmitError> {
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
    emit_insn(instruction, &context, -1, 2)
}

#[test]
fn unsupported_x87_forms_return_addressed_errors() {
    for (bytes, reason) in [
        ([0xdf, 0x20], "FBLD"),
        ([0xdf, 0x30], "FBSTP"),
        ([0xdb, 0xe0], "FPU opcode"),
    ] {
        let error = emit(&bytes).unwrap_err();
        assert_eq!(error.guest_va, 0x401000);
        assert!(error.message.contains(reason), "{error}");
    }
}

#[test]
fn supported_instructions_still_emit_and_guest_traps_remain_traps() {
    for bytes in [&[0xc3][..], &[0xd9, 0xe8], &[0xd9, 0x00], &[0xdf, 0x00]] {
        assert!(!emit(bytes).unwrap().is_empty());
    }
    assert_eq!(emit(&[0xcc]).unwrap(), [Op::UNREACHABLE]);
    assert_eq!(emit(&[0x0f, 0x0b]).unwrap().last(), Some(&Op::UNREACHABLE));
}

#[test]
fn invalid_x87_encoding_is_rejected_by_decoder() {
    let mut memory = GuestMem::new();
    memory.write(0, &[0xd9, 0x08]);
    assert!(decode_insn(&memory, 0x401000, 0x401000).is_err());
}

#[test]
fn missing_fpu_opcode_is_a_diagnostic() {
    let instruction = Insn {
        va: 0x401000,
        mnem: Mnem::Fpu,
        ..Insn::default()
    };
    let error = emit_decoded(&instruction).unwrap_err();
    assert_eq!(error.guest_va, 0x401000);
    assert!(error.message.contains("raw opcode"));
}
