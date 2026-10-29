//! Adapt iced-x86 decoding to the instruction forms supported by the emitter.
use super::mnemonic::{flags, mnemonic, string_width};
use super::operands::{memory, operand};
use super::types::{DResult, DecodeError, Insn, MemOp, Mnem, Operand, Rep};
use crate::guestmem::GuestMem;
use iced_x86::{Decoder, DecoderOptions, Instruction, Mnemonic, OpKind, Register};

/// Decode at most one architectural instruction from the sparse guest image.
pub fn decode_insn(image: &GuestMem, base: u32, va: u32) -> DResult<Insn> {
    let start = va
        .checked_sub(base)
        .ok_or_else(|| error(va, "address precedes image"))?;
    let mut bytes = [0; 15];
    let mut length = 0;
    for byte in &mut bytes {
        let Some(value) = image.get(start as usize + length) else {
            break;
        };
        *byte = value;
        length += 1;
    }
    let mut decoder = Decoder::with_ip(32, &bytes[..length], u64::from(va), DecoderOptions::NONE);
    let decoded = decoder.decode();
    if decoded.is_invalid() {
        return Err(error(va, "invalid x86 instruction"));
    }
    let opcode = opcode_offset(&bytes[..decoded.len()]);
    let prefixes = &bytes[..opcode];
    if prefixes.contains(&0x67) && decoded.mnemonic() != Mnemonic::Xlatb {
        return Err(error(va, "16-bit addressing is unsupported"));
    }
    if decoded.segment_prefix() == Register::GS {
        return Err(error(va, "GS segment addressing is unsupported"));
    }
    if prefixes.contains(&0x66)
        && matches!(
            decoded.mnemonic(),
            Mnemonic::Call | Mnemonic::Jmp | Mnemonic::Ret | Mnemonic::Leave
        )
    {
        return Err(error(va, "16-bit control-flow forms are unsupported"));
    }
    if (0xd8..=0xdf).contains(&bytes[opcode]) {
        return x87(&decoded, &bytes[opcode..], va);
    }
    let mnem = mnemonic(&decoded).ok_or_else(|| error(va, "unsupported x86 mnemonic"))?;
    let mut insn = Insn {
        va,
        size: decoded.len() as u32,
        mnem,
        flags: flags(mnem),
        ..Insn::default()
    };
    if let Some(width) = string_width(decoded.mnemonic()) {
        if decoded.segment_prefix() == Register::FS {
            return Err(error(va, "FS string operands are unsupported"));
        }
        insn.ops.push(Operand::Imm(width));
        insn.rep = if decoded.has_repne_prefix() {
            Rep::WhileNotEqual
        } else if decoded.has_rep_prefix() {
            Rep::WhileEqual
        } else {
            Rep::None
        };
    } else if mnem == Mnem::Xlat {
        insn.ops = vec![
            Operand::Mem(MemOp {
                base: 3,
                index: -1,
                scale: 1,
                disp: 0,
                size: 1,
                fs: decoded.segment_prefix() == Register::FS,
            }),
            Operand::Imm(if prefixes.contains(&0x67) { 16 } else { 32 }),
        ];
    } else if matches!(mnem, Mnem::Int3 | Mnem::Nop) {
        // Interrupts are guest traps; FWAIT and interrupt-mask changes are no-ops.
    } else {
        for index in 0..decoded.op_count() {
            if decoded.op_kind(index) == OpKind::NearBranch32 {
                insn.rel = Some(decoded.near_branch32());
            } else {
                insn.ops.push(operand(&decoded, index, va)?);
            }
        }
    }
    if prefixes.contains(&0x66)
        && (matches!(mnem, Mnem::Pushf | Mnem::Popf)
            || (mnem == Mnem::Push && matches!(insn.ops.first(), Some(Operand::Imm(_)))))
    {
        insn.ops.push(Operand::Imm(2));
    }
    if matches!(mnem, Mnem::Jcc | Mnem::Setcc) {
        insn.cc = Some(decoded.condition_code() as u8 - 1);
    }
    Ok(insn)
}

/// Prefix position is needed only for the emitter's raw x87 encoding contract.
fn opcode_offset(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .position(|byte| {
            !matches!(
                byte,
                0x66 | 0x67 | 0xf0 | 0xf2 | 0xf3 | 0x26 | 0x2e | 0x36 | 0x3e | 0x64 | 0x65
            )
        })
        .unwrap_or(0)
}

fn x87(decoded: &Instruction, opcode: &[u8], va: u32) -> DResult<Insn> {
    let raw = (i32::from(opcode[0]) << 8) | i32::from(opcode[1]);
    let operand = if opcode[1] & 0xc0 == 0xc0 {
        Operand::St(i32::from(opcode[1] & 7))
    } else {
        // The x87 emitter chooses access width from the raw opcode.
        Operand::Mem(memory(decoded, 4, va)?)
    };
    Ok(Insn {
        va,
        size: decoded.len() as u32,
        mnem: Mnem::Fpu,
        ops: vec![Operand::Imm(raw), operand],
        flags: flags(Mnem::Fpu),
        ..Insn::default()
    })
}

pub(super) fn error(va: u32, message: &'static str) -> DecodeError {
    DecodeError::new(message, va, None)
}

#[cfg(test)]
#[path = "../../tests/unit/x86/decode.rs"]
mod tests;
