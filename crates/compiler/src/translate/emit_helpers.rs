//! Shared instruction sequences for constants, locals, guest registers and memory.

use super::abi;
use crate::wasm::leb::sleb;
use crate::wasm::opcodes::Op;

pub type Seq = Vec<u8>;

/// Register globals are the first imported globals, indexed by x86 register number.
pub fn reg(r: u8) -> Seq {
    let mut v = vec![Op::GLOBAL_GET];
    v.extend(sleb(r as i32));
    v
}

pub fn set_reg(r: u8, value: Seq) -> Seq {
    let mut v = value;
    v.push(Op::GLOBAL_SET);
    v.extend(sleb(r as i32));
    v
}

/// Load a function-local slot, distinct from guest register globals.
pub fn local_get(i: u32) -> Seq {
    let mut v = vec![Op::LOCAL_GET];
    v.extend(sleb(i as i32));
    v
}

/// Store the top stack value into a function-local slot.
pub fn local_set(i: u32) -> Seq {
    let mut v = vec![Op::LOCAL_SET];
    v.extend(sleb(i as i32));
    v
}

pub fn i32c(v: i32) -> Seq {
    let mut v2 = vec![Op::I32_CONST];
    v2.extend(sleb(v));
    v2
}

pub fn i64c(v: i64) -> Seq {
    let mut v2 = vec![Op::I64_CONST];
    v2.extend(crate::wasm::leb::sleb64(v));
    v2
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

fn load_op(size: u8) -> u8 {
    match size {
        1 => Op::I32_LOAD8_U,
        2 => Op::I32_LOAD16_U,
        _ => Op::I32_LOAD,
    }
}

fn store_op(size: u8) -> u8 {
    match size {
        1 => Op::I32_STORE8,
        2 => Op::I32_STORE16,
        _ => Op::I32_STORE,
    }
}

fn memarg(align: u32) -> Seq {
    let mut v = sleb(align as i32);
    v.extend(sleb(0));
    v
}

/// Emit a memory read of the operand's size (zero-extended).
pub fn emit_load(m: &crate::x86::types::MemOp) -> Seq {
    let mut v = emit_address(m);
    v.push(load_op(m.size));
    v.extend(memarg(if m.size == 4 {
        2
    } else if m.size == 2 {
        1
    } else {
        0
    }));
    v
}

/// Emit a memory write; wasm stack order is [addr, value] with value on top.
/// (value is materialized first, masked into TMP2=10, then addr, then store.)
pub fn emit_store(m: &crate::x86::types::MemOp, value: Seq) -> Seq {
    let val_tmp = 10u32; // TMP2 reserved in the function local layout
    let mut v = value;
    v.extend(size_mask(m.size as u32));
    v.push(Op::LOCAL_SET);
    v.extend(sleb(val_tmp as i32));
    v.extend(emit_address(m));
    v.push(Op::LOCAL_GET);
    v.extend(sleb(val_tmp as i32));
    v.push(store_op(m.size));
    v.extend(memarg(if m.size == 4 {
        2
    } else if m.size == 2 {
        1
    } else {
        0
    }));
    v
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
