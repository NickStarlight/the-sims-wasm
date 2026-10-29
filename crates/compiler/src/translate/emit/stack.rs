//! Operand-size-aware stack operations in the guest's 32-bit stack segment.
use super::{
    abi, emit_load, emit_store, i32c, local_get, local_set, mem_at, read_op, reg, set_reg,
    write_op, Insn, Op, Operand, Seq, ESP, T1,
};

fn operand_width(insn: &Insn) -> u8 {
    match insn.ops.first() {
        Some(Operand::Reg { size: 2, .. })
        | Some(Operand::Mem(crate::x86::types::MemOp { size: 2, .. })) => 2,
        // Immediate operands have no size field; the decoder supplies the stack width.
        Some(Operand::Imm(_)) if insn.ops.get(1) == Some(&Operand::Imm(2)) => 2,
        _ => 4,
    }
}

fn flags_width(insn: &Insn) -> u8 {
    if insn.ops.first() == Some(&Operand::Imm(2)) {
        2
    } else {
        4
    }
}

fn adjust_sp(bytes: i32) -> Seq {
    let mut value = reg(ESP);
    value.extend(i32c(bytes));
    value.push(Op::I32_ADD);
    set_reg(ESP, value)
}

pub(super) fn emit_push(insn: &Insn) -> Seq {
    let width = operand_width(insn);
    // PUSH SP/ESP and memory sources must read the old stack pointer.
    let mut out = read_op(&insn.ops[0]);
    out.extend(local_set(T1));
    out.extend(adjust_sp(-i32::from(width)));
    out.extend(emit_store(&mem_at(ESP, 0, width), local_get(T1)));
    out
}

pub(super) fn emit_pop(insn: &Insn) -> Seq {
    let width = operand_width(insn);
    let mut out = emit_load(&mem_at(ESP, 0, width));
    out.extend(local_set(T1));
    // POP [ESP] computes its destination after the increment; POP ESP replaces
    // the incremented pointer. Word register writes preserve the remaining bits.
    out.extend(adjust_sp(i32::from(width)));
    out.extend(write_op(&insn.ops[0], local_get(T1)));
    out
}

pub(super) fn emit_push_flags(insn: &Insn) -> Seq {
    let width = flags_width(insn);
    let mut out = adjust_sp(-i32::from(width));
    out.extend(emit_store(&mem_at(ESP, 0, width), reg(abi::FLAGS)));
    out
}

pub(super) fn emit_pop_flags(insn: &Insn) -> Seq {
    let width = flags_width(insn);
    let mut flags = emit_load(&mem_at(ESP, 0, width));
    if width == 2 {
        flags.extend(reg(abi::FLAGS));
        flags.extend(i32c(0xffff_0000u32 as i32));
        flags.push(Op::I32_AND);
        flags.push(Op::I32_OR);
    }
    let mut out = set_reg(abi::FLAGS, flags);
    out.extend(adjust_sp(i32::from(width)));
    out
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/emit/stack.rs"]
mod tests;
