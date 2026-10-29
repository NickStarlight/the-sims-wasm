//! Fuse RGB565 blend calculations and guarded spans between checkpoints.
//! The owning routine's complete fingerprint is verified by `recognize`.
//! Spans cannot cross an observable checkpoint. Guards retain the original
//! scalar translation for aliases, invalid data and partial-fault behavior.

use super::*;
use crate::translate::{emit::emit_insn, emit_fpu::runs::Run};
use crate::x86::types::Insn;

const ALPHA: i32 = 0x6e5c0c;
const SOURCE: i32 = 0x6e5c4c;
const PREVIOUS: i32 = 0x6e5d0c;
const BLEND: i32 = 0x6e5e5c;
const DATA_BYTES: i32 = BLEND + 32768 - ALPHA;

// Otherwise-unused i32 scratch slots in layout::LOCAL_RUNS. The architectural
// register promotion pass uses separate locals starting at CACHED_REGISTERS.
const INDEX: u32 = 4;
const DESTINATION: u32 = 5;
const COUNT: u32 = 6;

mod alpha;

pub(super) fn loop_kernel(
    address: u32,
    context: &super::super::emit::EmitCtx,
    depth: i32,
) -> Option<Seq> {
    if let Some(code) = alpha::loop_kernel(address, context, depth) {
        return Some(code);
    }
    if address != 0x499c44 {
        return None;
    }
    let successor = *context.id.get(&0x499cd9)?;
    Some(full_span(successor, depth, context.block_trace.is_some()))
}

/// The all-blended 32-pixel loop has one basic-block tick per pixel. Validate
/// every remaining access before changing architectural state, then consume
/// only a span that ends strictly before the next original checkpoint.
fn full_span(successor: i32, depth: i32, checkpointed: bool) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    guard(&mut code, binary(reg(abi::EDI), i32c(31), Op::I32_EQ));
    guard(&mut code, binary(reg(abi::EDX), i32c(64), Op::I32_LT_U));
    guard(
        &mut code,
        binary(reg(abi::EDX), i32c(1), Op::I32_AND)
            .into_iter()
            .chain([Op::I32_EQZ])
            .collect(),
    );
    code.extend(binary(
        binary(i32c(64), reg(abi::EDX), Op::I32_SUB),
        i32c(1),
        Op::I32_SHR_U,
    ));
    code.extend(local_set(COUNT));
    if checkpointed {
        // The enclosing segment already consumed its first tick.
        guard(
            &mut code,
            binary(
                local_get(COUNT),
                reg(super::super::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                Op::I32_LE_U,
            ),
        );
    }
    guard(&mut code, within(i32c(ALPHA), i64::from(DATA_BYTES)));
    guard(&mut code, binary(reg(abi::EBP), i32c(24), Op::I32_GE_U));
    let frame = binary(reg(abi::EBP), i32c(-24), Op::I32_ADD);
    guard(&mut code, within(frame.clone(), 52));
    guard(
        &mut code,
        disjoint(frame.clone(), i32c(52), i32c(ALPHA), i32c(DATA_BYTES)),
    );
    guard(&mut code, binary(reg(abi::ESP), i32c(4), Op::I32_GE_U));
    let pushed = binary(reg(abi::ESP), i32c(-4), Op::I32_ADD);
    guard(&mut code, within(pushed.clone(), 4));
    guard(
        &mut code,
        disjoint(pushed.clone(), i32c(4), i32c(ALPHA), i32c(DATA_BYTES)),
    );
    guard(
        &mut code,
        disjoint(pushed.clone(), i32c(4), frame.clone(), i32c(52)),
    );
    let destination_slot = binary(reg(abi::EBP), i32c(24), Op::I32_ADD);
    code.extend(load(destination_slot.clone(), Op::I32_LOAD, 2));
    code.extend(local_set(DESTINATION));
    let bytes = binary(local_get(COUNT), i32c(2), Op::I32_MUL);
    guard(
        &mut code,
        within_length(local_get(DESTINATION), extend(bytes.clone())),
    );
    for (address, length) in [
        (i32c(ALPHA), i32c(DATA_BYTES)),
        (frame.clone(), i32c(52)),
        (pushed.clone(), i32c(4)),
    ] {
        guard(
            &mut code,
            disjoint(local_get(DESTINATION), bytes.clone(), address, length),
        );
    }
    // Out-of-range alpha changes signed-index arithmetic in the native code.
    // Check the complete span while all guest state is still untouched.
    code.extend(reg(abi::EDX));
    code.extend(local_set(INDEX));
    code.extend([Op::LOOP, 0x40]);
    code.extend(load(
        binary(i32c(ALPHA), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(i32c(255));
    code.extend([Op::I32_GT_U, Op::BR_IF, 1]);
    code.extend(binary(local_get(INDEX), i32c(2), Op::I32_ADD));
    code.extend(local_set(INDEX));
    code.extend(binary(local_get(INDEX), i32c(64), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);

    code.extend(reg(abi::EDX));
    code.extend(local_set(INDEX));
    code.extend([Op::LOOP, 0x40]);
    code.extend(load(
        binary(i32c(ALPHA), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(i32c(3));
    code.push(Op::I32_SHR_U);
    code.extend(i32c(5));
    code.push(Op::I32_SHL);
    code.extend(local_set(T1));
    for (local, address) in [(A, SOURCE), (B, PREVIOUS)] {
        code.extend(load(
            binary(i32c(address), local_get(INDEX), Op::I32_ADD),
            Op::I32_LOAD16_U,
            1,
        ));
        code.extend(local_set(local));
    }
    code.extend(channels());
    code.extend(local_get(DESTINATION));
    code.extend(local_get(RES));
    code.extend([Op::I32_STORE16, 1, 0]);
    for local in [INDEX, DESTINATION] {
        code.extend(binary(local_get(local), i32c(2), Op::I32_ADD));
        code.extend(local_set(local));
    }
    code.extend(binary(local_get(INDEX), i32c(64), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);

    // No checkpoint, trap or alias can observe the intermediate scratch writes.
    code.extend(store(pushed, i32c(31)));
    code.extend(store(frame, local_get(T1)));
    code.extend(store(destination_slot, local_get(DESTINATION)));
    code.extend(set_reg(
        abi::EAX,
        binary(local_get(DESTINATION), i32c(-2), Op::I32_ADD),
    ));
    for (register, local) in [(abi::ECX, B), (abi::EBX, T2), (abi::ESI, RES)] {
        code.extend(set_reg(register, local_get(local)));
    }
    code.extend(set_reg(abi::EDX, i32c(64)));
    code.extend(set_reg(abi::FLAGS, i32c(abi::ZF | abi::PF)));
    if checkpointed {
        code.extend(set_reg(
            super::super::scheduling::CHECKPOINT_BUDGET_GLOBAL,
            binary(
                reg(super::super::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                binary(local_get(COUNT), i32c(1), Op::I32_SUB),
                Op::I32_SUB,
            ),
        ));
    }
    code.extend(i32c(successor));
    code.extend(local_set(super::super::layout::NEXT));
    code.push(Op::BR);
    code.extend(crate::wasm::leb::uleb(depth as u32 + 1));
    code.push(Op::END);
    code
}

pub(super) fn sequence_end(address: u32) -> Option<u32> {
    match address {
        0x499c44 => Some(0x499cc4),
        0x499d1c => Some(0x499d9c),
        0x499ec0 => Some(0x499f41),
        _ => None,
    }
}

pub(super) fn emit(instructions: &[&Insn], context: &super::super::emit::EmitCtx) -> Option<Run> {
    let start = instructions.first()?.va;
    let end = sequence_end(start)?;
    let count = instructions.iter().take_while(|insn| insn.va < end).count();
    let last = instructions.get(count.checked_sub(1)?)?;
    // Never span a segment entry or cooperative checkpoint.
    if last.va + last.size != end {
        return None;
    }
    let mut code = prefix(start == 0x499ec0);
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

/// Ends after the guard block; caller appends the scalar fallback and outer END.
fn prefix(tail: bool) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40, Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    guard(&mut code, binary(reg(abi::EDI), i32c(31), Op::I32_EQ));
    guard(&mut code, within(i32c(ALPHA), i64::from(DATA_BYTES)));
    let weight_offset = if tail { 40 } else { 24 };
    guard(
        &mut code,
        binary(reg(abi::EBP), i32c(weight_offset), Op::I32_GE_U),
    );
    let weight_slot = binary(reg(abi::EBP), i32c(-weight_offset), Op::I32_ADD);
    guard(
        &mut code,
        within(weight_slot.clone(), i64::from(weight_offset)),
    );
    guard(
        &mut code,
        disjoint(weight_slot.clone(), i32c(4), i32c(ALPHA), i32c(DATA_BYTES)),
    );
    let pushed = binary(reg(abi::ESP), i32c(-4), Op::I32_ADD);
    if !tail {
        guard(&mut code, binary(reg(abi::ESP), i32c(4), Op::I32_GE_U));
        guard(&mut code, within(pushed.clone(), 4));
        guard(
            &mut code,
            disjoint(pushed.clone(), i32c(4), i32c(ALPHA), i32c(DATA_BYTES)),
        );
        guard(
            &mut code,
            disjoint(pushed.clone(), i32c(4), weight_slot.clone(), i32c(4)),
        );
    }
    let index = if tail {
        let index = load(
            binary(reg(abi::EBP), i32c(-8), Op::I32_ADD),
            Op::I32_LOAD,
            2,
        );
        guard(&mut code, binary(index.clone(), i32c(32), Op::I32_LT_U));
        binary(index, i32c(2), Op::I32_MUL)
    } else {
        guard(&mut code, binary(reg(abi::EDX), i32c(64), Op::I32_LT_U));
        guard(
            &mut code,
            binary(reg(abi::EDX), i32c(1), Op::I32_AND)
                .into_iter()
                .chain([Op::I32_EQZ])
                .collect(),
        );
        reg(abi::EDX)
    };
    code.extend(index);
    code.extend(local_set(T3));
    code.extend(load(
        binary(i32c(ALPHA), local_get(T3), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(local_set(T1));
    guard(&mut code, binary(local_get(T1), i32c(255), Op::I32_LE_U));
    code.extend(binary(
        binary(local_get(T1), i32c(3), Op::I32_SHR_U),
        i32c(5),
        Op::I32_SHL,
    ));
    code.extend(local_set(T1)); // quantized alpha * 32
    for (local, address) in [(A, SOURCE), (B, PREVIOUS)] {
        code.extend(load(
            binary(i32c(address), local_get(T3), Op::I32_ADD),
            Op::I32_LOAD16_U,
            1,
        ));
        code.extend(local_set(local));
    }
    if !tail {
        code.extend(store(pushed, i32c(31)));
    }
    code.extend(store(weight_slot, local_get(T1)));

    code.extend(channels());
    for (register, local) in [
        (abi::EAX, A),
        (abi::ECX, B),
        (abi::EBX, T2),
        (abi::ESI, RES),
    ] {
        code.extend(set_reg(register, local_get(local)));
    }
    if tail {
        code.extend(set_reg(
            abi::EDX,
            binary(local_get(T3), i32c(5), Op::I32_SHL),
        ));
    }
    code.extend(flag_logic());
    code.extend([Op::BR, 1, Op::END]);
    code
}

// Inputs: A/B are source/previous RGB565 and T1 is quantized alpha times32.
// Outputs retain the native last-pixel register intermediates as well as RES.
fn channels() -> Seq {
    let mut code = Vec::new();
    let component = |local, shift| {
        binary(
            binary(local_get(local), i32c(shift), Op::I32_SHR_U),
            i32c(31),
            Op::I32_AND,
        )
    };
    let row = |shift| {
        binary(
            binary(component(A, shift), local_get(T1), Op::I32_ADD),
            i32c(5),
            Op::I32_SHL,
        )
    };
    let sample = |index| load(binary(i32c(BLEND), index, Op::I32_ADD), Op::I32_LOAD8_U, 0);
    code.extend(row(11));
    code.extend(local_set(T2)); // EBX retains the red lookup row.
    code.extend(sample(binary(local_get(T2), component(B, 11), Op::I32_ADD)));
    code.extend(local_set(T3)); // red result
    code.extend(sample(binary(row(6), component(B, 6), Op::I32_ADD)));
    code.extend(binary(local_get(T3), i32c(5), Op::I32_SHL));
    code.push(Op::I32_OR);
    code.extend(i32c(6));
    code.push(Op::I32_SHL);
    code.extend(local_set(RES));
    code.extend(sample(binary(row(0), component(B, 0), Op::I32_ADD)));
    code.extend(local_set(A));
    code.extend(binary(local_get(B), i32c(31), Op::I32_AND));
    code.extend(local_set(B));
    code.extend(binary(local_get(RES), local_get(A), Op::I32_OR));
    code.extend(local_set(RES));
    code
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/kernels/compositor.rs"]
mod tests;
