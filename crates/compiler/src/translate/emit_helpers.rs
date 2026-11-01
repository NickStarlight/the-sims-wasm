//! Shared instruction sequences for constants, locals, guest registers and memory.

use super::abi;
use crate::wasm::opcodes::Op;
use wasm_encoder::{Encode, Instruction, MemArg};

pub type Seq = Vec<u8>;

fn instruction(value: Instruction<'_>) -> Seq {
    let mut bytes = Vec::with_capacity(8);
    value.encode(&mut bytes);
    bytes
}

/// Register globals are the first imported globals, indexed by x86 register number.
pub fn reg(r: u8) -> Seq {
    instruction(Instruction::GlobalGet(r as u32))
}

pub fn set_reg(r: u8, mut value: Seq) -> Seq {
    Instruction::GlobalSet(r as u32).encode(&mut value);
    value
}

/// Load a function-local slot, distinct from guest register globals.
pub fn local_get(i: u32) -> Seq {
    instruction(Instruction::LocalGet(i))
}

/// Store the top stack value into a function-local slot.
pub fn local_set(i: u32) -> Seq {
    instruction(Instruction::LocalSet(i))
}

pub fn i32c(v: i32) -> Seq {
    instruction(Instruction::I32Const(v))
}

pub fn i64c(v: i64) -> Seq {
    instruction(Instruction::I64Const(v))
}

/// Emit addressing computation → i32 on the stack.
pub fn emit_address(m: &crate::x86::types::MemOp) -> Seq {
    let mut out = if m.base == -1 {
        Vec::new()
    } else {
        reg(m.base as u8)
    };
    if m.index != -1 {
        let mut index = reg(m.index as u8);
        if m.scale != 1 {
            index.extend(i32c((m.scale as f64).log2() as i32));
            index.push(Op::I32_SHL);
        }
        if out.is_empty() {
            out = index;
        } else {
            out.extend(index);
            out.push(Op::I32_ADD);
        }
    }
    if out.is_empty() {
        out = i32c(m.disp);
    } else if m.disp != 0 {
        out.extend(i32c(m.disp));
        out.push(Op::I32_ADD);
    }
    if m.fs {
        out.extend(reg(abi::TEB_BASE));
        out.push(Op::I32_ADD);
    }
    out
}

fn memarg(size: u8) -> MemArg {
    MemArg {
        offset: 0,
        align: if size == 4 {
            2
        } else if size == 2 {
            1
        } else {
            0
        },
        memory_index: 0,
    }
}

/// Emit a memory read of the operand's size (zero-extended).
pub fn emit_load(m: &crate::x86::types::MemOp) -> Seq {
    let mut v = emit_address(m);
    let argument = memarg(m.size);
    match m.size {
        1 => Instruction::I32Load8U(argument),
        2 => Instruction::I32Load16U(argument),
        _ => Instruction::I32Load(argument),
    }
    .encode(&mut v);
    v
}

/// Materialize value before address; TMP2 preserves x86 operand evaluation order.
pub fn emit_store(m: &crate::x86::types::MemOp, mut value: Seq) -> Seq {
    value.extend(size_mask(m.size as u32));
    Instruction::LocalSet(10).encode(&mut value);
    value.extend(emit_address(m));
    Instruction::LocalGet(10).encode(&mut value);
    let argument = memarg(m.size);
    match m.size {
        1 => Instruction::I32Store8(argument),
        2 => Instruction::I32Store16(argument),
        _ => Instruction::I32Store(argument),
    }
    .encode(&mut value);
    value
}

pub fn size_mask(size: u32) -> Seq {
    if size == 4 {
        Vec::new()
    } else {
        let mut v = i32c(if size == 1 { 0xff } else { 0xffff });
        v.push(Op::I32_AND);
        v
    }
}
