//! Convert supported iced-x86 operands without changing their width or address mode.
use super::decode::error;
use super::types::{DResult, MemOp, Operand};
use iced_x86::{Instruction, OpKind, Register};

pub(super) fn operand(insn: &Instruction, index: u32, va: u32) -> DResult<Operand> {
    Ok(match insn.op_kind(index) {
        OpKind::Register => {
            let (r, size) = register(insn.op_register(index))
                .ok_or_else(|| error(va, "unsupported register class"))?;
            Operand::Reg { r, size }
        }
        OpKind::Memory => {
            let size = if insn.mnemonic() == iced_x86::Mnemonic::Lea {
                register(insn.op0_register())
                    .map(|(_, size)| size)
                    .unwrap_or(0)
            } else {
                insn.memory_size().size() as u8
            };
            if !matches!(size, 1 | 2 | 4) {
                return Err(error(va, "unsupported memory width"));
            }
            Operand::Mem(memory(insn, size, va)?)
        }
        OpKind::Immediate8
        | OpKind::Immediate16
        | OpKind::Immediate32
        | OpKind::Immediate8to16
        | OpKind::Immediate8to32 => Operand::Imm(insn.immediate(index) as i32),
        _ => return Err(error(va, "unsupported operand kind")),
    })
}

pub(super) fn memory(insn: &Instruction, size: u8, va: u32) -> DResult<MemOp> {
    let address_reg = |reg| -> DResult<i8> {
        if reg == Register::None {
            return Ok(-1);
        }
        match register(reg) {
            Some((r, 4)) => Ok(r as i8),
            _ => Err(error(va, "unsupported address register")),
        }
    };
    Ok(MemOp {
        base: address_reg(insn.memory_base())?,
        index: address_reg(insn.memory_index())?,
        scale: insn.memory_index_scale() as u8,
        disp: insn.memory_displacement32() as i32,
        size,
        fs: insn.segment_prefix() == Register::FS,
    })
}

fn register(reg: Register) -> Option<(u8, u8)> {
    use Register::*;
    let registers = [
        [AL, CL, DL, BL, AH, CH, DH, BH],
        [AX, CX, DX, BX, SP, BP, SI, DI],
        [EAX, ECX, EDX, EBX, ESP, EBP, ESI, EDI],
    ];
    registers.iter().zip([1, 2, 4]).find_map(|(group, size)| {
        group
            .iter()
            .position(|&candidate| candidate == reg)
            .map(|r| (r as u8, size))
    })
}
