//! Preserve all 64 integer bits in balanced x87 copy sequences.
//!
//! FILD m64int is exact in an x87 extended register. Converting it through our
//! binary64 arithmetic bank loses low bits, so native memcpy implementations
//! using FILD / FXCH / FISTP must retain the original integer separately. Only
//! uninterrupted, balanced sequences that never inspect an incoming ST value
//! qualify. Arithmetic and values escaping the sequence use the numeric path.

use super::*;
use crate::translate::layout::FP_LOCALS;
use crate::x86::types::{Mnem, Operand};

enum CopyOp<'a> {
    Load(&'a MemOp),
    Store(&'a MemOp),
    Swap(usize),
}

fn decode(insn: &Insn) -> Option<CopyOp<'_>> {
    if insn.mnem != Mnem::Fpu {
        return None;
    }
    let Operand::Imm(raw) = insn.ops.first()? else {
        return None;
    };
    let raw = *raw as u32;
    if raw & 0xfff8 == 0xd9c8 {
        return Some(CopyOp::Swap((raw & 7) as usize));
    }
    if raw >> 8 != 0xdf || raw & 0xc0 == 0xc0 {
        return None;
    }
    let Operand::Mem(m) = insn.ops.get(1)? else {
        return None;
    };
    match (raw >> 3) & 7 {
        5 => Some(CopyOp::Load(m)),
        7 => Some(CopyOp::Store(m)),
        _ => None,
    }
}

pub(crate) fn emit(insns: &[&Insn]) -> Option<runs::Run> {
    let mut operations = Vec::new();
    let mut depth = 0;
    let mut count = 0;
    for insn in insns {
        let Some(operation) = decode(insn) else {
            break;
        };
        match operation {
            CopyOp::Load(_) if depth < 8 => depth += 1,
            CopyOp::Store(_) if depth > 0 => depth -= 1,
            CopyOp::Swap(i) if i < depth => {}
            _ => break,
        }
        operations.push(operation);
        if depth == 0 {
            count = operations.len();
        }
    }
    if count == 0 {
        return None;
    }
    operations.truncate(count);

    // The bank is implementation memory, but retain the ordinary emitter's
    // behavior for operands aliasing it. Also retain partial fault behavior by
    // taking that path if any access would exceed linear memory.
    let mut code = reg(abi::TEB_BASE);
    code.extend(i32c(abi::FP_OFFSET));
    code.push(Op::I32_ADD);
    code.extend(local_set(T1));
    code.extend([0x3f, 0, Op::I64_EXTEND_I32_U]);
    code.extend(i64c(16));
    code.push(Op::I64_SHL);
    code.extend(local_set(FULL));
    code.extend(i32c(1));
    let mut ranges = Vec::new();
    for operation in &operations {
        let (CopyOp::Load(m) | CopyOp::Store(m)) = operation else {
            continue;
        };
        if !ranges.contains(m) {
            ranges.push(*m);
            code.extend(runs::guard(m, 8));
            code.push(Op::I32_AND);
        }
    }
    code.extend([Op::IF, 0x40]);
    let mut offsets = vec![(0, 0)];
    depth = 0;
    for (index, operation) in operations.iter().enumerate() {
        offsets.push((index, code.len() as u32));
        match operation {
            CopyOp::Load(m) => {
                code.extend(push_top());
                code.extend(emit_address(m));
                code.extend([Op::I64_LOAD, 3, 0]);
                code.extend(local_set(FULL));
                // Reinterpretation is bit-preserving even for NaN-shaped data.
                code.extend(local_get(FULL));
                code.push(Op::F64_REINTERPRET_I64);
                code.extend(local_set(FP_LOCALS + depth as u32));
                let mut numeric = local_get(FULL);
                numeric.push(Op::F64_CONVERT_I64_S);
                code.extend(store_f64(slot(0), numeric));
                depth += 1;
            }
            CopyOp::Store(m) => {
                depth -= 1;
                code.extend(emit_address(m));
                code.extend(local_get(FP_LOCALS + depth as u32));
                code.push(Op::I64_REINTERPRET_F64);
                code.extend([Op::I64_STORE, 3, 0]);
                code.extend(pop_top());
            }
            CopyOp::Swap(i) => {
                code.extend(fxch(*i as i32));
                let top = FP_LOCALS + depth as u32 - 1;
                let other = top - *i as u32;
                code.extend(local_get(top));
                code.extend(local_set(FTMP));
                code.extend(local_get(other));
                code.extend(local_set(top));
                code.extend(local_get(FTMP));
                code.extend(local_set(other));
            }
        }
    }
    code.push(Op::ELSE);
    for (index, insn) in insns.iter().take(count).enumerate() {
        offsets.push((index, code.len() as u32));
        code.extend(emit_fpu(insn).ok()?);
    }
    code.push(Op::END);
    Some(runs::Run {
        count,
        code,
        offsets,
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/emit_fpu/integer_copy.rs"]
mod tests;
