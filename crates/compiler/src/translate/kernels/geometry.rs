//! Fuse verified coordinate calculations between native checkpoints.
//! Preflight every access before omitting intermediate register/flag writes;
//! potential faults retain the instruction-by-instruction translation.
use super::*;
use crate::translate::{emit::emit_insn, emit_fpu::runs::Run};
use crate::x86::types::Insn;

const Y: u32 = 4;
const OFFSET: u32 = 5;
const LOOKUP: u32 = 6;

pub(super) fn sequence_end(address: u32) -> Option<u32> {
    match address {
        0x435007 => Some(0x43502f),
        _ => None,
    }
}

pub(super) fn emit(instructions: &[&Insn], context: &super::super::emit::EmitCtx) -> Option<Run> {
    let start = instructions.first()?.va;
    let end = sequence_end(start)?;
    let count = instructions.iter().take_while(|insn| insn.va < end).count();
    let last = instructions.get(count.checked_sub(1)?)?;
    if last.va + last.size != end {
        return None;
    }
    let mut code = vec![Op::BLOCK, 0x40, Op::BLOCK, 0x40];
    transform(&mut code, context.memory_minimum_pages);
    code.extend([Op::BR, 1, Op::END]);
    let mut offsets = vec![(0, 0)];
    for (index, instruction) in instructions.iter().take(count).enumerate() {
        offsets.push((index, code.len() as u32));
        code.extend(emit_insn(instruction, context, -1, 0).ok()?);
    }
    code.push(Op::END);
    Some(Run {
        count,
        code,
        offsets,
    })
}

fn at(register: u8, offset: i32) -> Seq {
    binary(reg(register), i32c(offset), Op::I32_ADD)
}

fn minimum_guard(code: &mut Seq, address: Seq, minimum_pages: u32) {
    code.extend(extend(address));
    code.extend(i64c(2));
    code.push(Op::I64_ADD);
    code.extend(i64c(i64::from(minimum_pages) * 65536));
    code.extend([0x58, Op::I32_EQZ, Op::BR_IF, 0]);
}

fn transform(code: &mut Seq, minimum_pages: u32) {
    // Stack-local coordinates and the fixed lookup table normally lie inside
    // the module's guaranteed initial memory. Larger addresses use the scalar
    // fallback; the optimization does not need to query the current size.
    minimum_guard(code, reg(abi::EAX), minimum_pages);
    code.extend(load(at(abi::EAX, 1), 0x2c, 0)); // i32.load8_s
    code.extend(local_set(Y));
    let x = load(reg(abi::EAX), 0x2c, 0);
    let row = binary(
        binary(binary(reg(abi::ECX), i32c(6), Op::I32_SHL), x, Op::I32_ADD),
        i32c(-64),
        Op::I32_ADD,
    );
    code.extend(binary(
        binary(row, i32c(6), Op::I32_SHL),
        local_get(Y),
        Op::I32_ADD,
    ));
    code.extend(local_set(OFFSET));
    code.extend(binary(local_get(OFFSET), i32c(1), Op::I32_SHL));
    code.extend(local_set(LOOKUP));
    minimum_guard(
        code,
        binary(local_get(LOOKUP), i32c(0x6de81c), Op::I32_ADD),
        minimum_pages,
    );
    // Use the existing final shift's flag semantics, including undefined bits.
    code.extend(local_get(OFFSET));
    code.extend(local_set(A));
    code.extend(i32c(1));
    code.extend(local_set(B));
    code.extend(crate::translate::emit_flags::flag_shift("shl"));
    code.extend(set_reg(abi::EDX, local_get(LOOKUP)));
    code.extend(set_reg(abi::ECX, local_get(Y)));
    // Preserve native read/write ordering even when the destination aliases
    // either lookup byte. In particular, do not combine these into a u16 load.
    for offset in [0, 1] {
        let byte = load(at(abi::EDX, 0x6de81c + offset), Op::I32_LOAD8_U, 0);
        code.extend(set_reg(
            abi::ECX,
            binary(
                binary(reg(abi::ECX), i32c(-256), Op::I32_AND),
                byte,
                Op::I32_OR,
            ),
        ));
        code.extend(at(abi::EAX, offset));
        code.extend(reg(abi::ECX));
        code.extend([Op::I32_STORE8, 0, 0]);
    }
}
