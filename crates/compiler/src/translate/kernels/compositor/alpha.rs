//! The separately-addressed alpha plane compositor verified at 0x498830.
//! The all-blended span uses only the low alpha byte, unlike its masked paths.
use super::*;

const SOURCE_POINTER: u32 = 7;
const DESTINATION_OFFSET: u32 = 8;
const MASK: u32 = 12;

pub(super) fn loop_kernel(
    address: u32,
    context: &super::super::super::emit::EmitCtx,
    depth: i32,
) -> Option<Seq> {
    if address == 0x4989ff {
        return Some(masked_span(
            *context.id.get(&0x498aa3)?,
            depth,
            context.block_trace.is_some(),
        ));
    }
    if address != 0x49892f {
        return None;
    }
    Some(full_span(
        *context.id.get(&0x4989c4)?,
        depth,
        context.block_trace.is_some(),
    ))
}

fn selected() -> Seq {
    binary(
        binary(
            local_get(MASK),
            binary(local_get(INDEX), i32c(1), Op::I32_SHR_U),
            Op::I32_SHR_U,
        ),
        i32c(1),
        Op::I32_AND,
    )
}

/// A masked pixel visits its test, then either a combined blend/increment
/// block or the skipped-pixel increment block. The discovered blocks overlap,
/// so both paths consume exactly two ticks, with different final trap markers.
fn masked_span(successor: i32, depth: i32, checkpointed: bool) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    guard(&mut code, binary(reg(abi::ESI), i32c(64), Op::I32_LT_U));
    guard(
        &mut code,
        binary(reg(abi::ESI), i32c(1), Op::I32_AND)
            .into_iter()
            .chain([Op::I32_EQZ])
            .collect(),
    );
    code.extend(binary(
        binary(i32c(64), reg(abi::ESI), Op::I32_SUB),
        i32c(1),
        Op::I32_SHR_U,
    ));
    code.extend(local_set(COUNT));
    cache_bounds(&mut code);
    guard(&mut code, within(i32c(ALPHA), i64::from(DATA_BYTES)));
    guard(&mut code, binary(reg(abi::EBP), i32c(40), Op::I32_GE_U));
    let frame = binary(reg(abi::EBP), i32c(-40), Op::I32_ADD);
    guard(&mut code, within(frame.clone(), 72));
    guard(
        &mut code,
        disjoint(frame.clone(), i32c(72), i32c(ALPHA), i32c(DATA_BYTES)),
    );
    let bit_slot = binary(reg(abi::EBP), i32c(-12), Op::I32_ADD);
    guard(
        &mut code,
        binary(
            load(bit_slot.clone(), Op::I32_LOAD, 2),
            binary(
                i32c(1),
                binary(reg(abi::ESI), i32c(1), Op::I32_SHR_U),
                Op::I32_SHL,
            ),
            Op::I32_EQ,
        ),
    );
    code.extend(load(
        binary(reg(abi::EBP), i32c(28), Op::I32_ADD),
        Op::I32_LOAD,
        2,
    ));
    code.extend(local_set(MASK));
    let source_slot = binary(reg(abi::EBP), i32c(-8), Op::I32_ADD);
    code.extend(load(source_slot.clone(), Op::I32_LOAD, 2));
    code.extend(local_set(SOURCE_POINTER));
    code.extend(load(
        binary(reg(abi::EBP), i32c(8), Op::I32_ADD),
        Op::I32_LOAD,
        2,
    ));
    code.extend(local_set(DESTINATION_OFFSET));
    code.extend(binary(
        local_get(SOURCE_POINTER),
        local_get(DESTINATION_OFFSET),
        Op::I32_ADD,
    ));
    code.extend(local_set(DESTINATION));
    let bytes = binary(local_get(COUNT), i32c(2), Op::I32_MUL);
    for pointer in [SOURCE_POINTER, DESTINATION] {
        guard(
            &mut code,
            within_length(local_get(pointer), extend(bytes.clone())),
        );
        guard(
            &mut code,
            disjoint(local_get(pointer), bytes.clone(), frame.clone(), i32c(72)),
        );
    }
    for (address, length) in [
        (i32c(ALPHA), i32c(DATA_BYTES)),
        (local_get(SOURCE_POINTER), bytes.clone()),
    ] {
        guard(
            &mut code,
            disjoint(local_get(DESTINATION), bytes.clone(), address, length),
        );
    }
    // COUNT now records basic-block visits rather than pixels.
    code.extend(binary(local_get(COUNT), i32c(2), Op::I32_MUL));
    code.extend(local_set(COUNT));
    if checkpointed {
        guard(
            &mut code,
            binary(
                local_get(COUNT),
                reg(crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                Op::I32_LE_U,
            ),
        );
    }
    code.extend(reg(abi::ESI));
    code.extend(local_set(INDEX));
    code.extend([Op::LOOP, 0x40]);
    code.extend(selected());
    code.extend([Op::IF, 0x40]);
    code.extend(load(
        binary(i32c(ALPHA), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(i32c(255));
    code.extend([Op::I32_GT_U, Op::BR_IF, 2, Op::END]);
    code.extend(binary(local_get(INDEX), i32c(2), Op::I32_ADD));
    code.extend(local_set(INDEX));
    code.extend(binary(local_get(INDEX), i32c(64), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);

    code.extend(reg(abi::ESI));
    code.extend(local_set(INDEX));
    code.extend([Op::LOOP, 0x40]);
    code.extend(selected());
    code.extend([Op::IF, 0x40]);
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
    code.extend(load(local_get(SOURCE_POINTER), Op::I32_LOAD16_U, 1));
    code.extend(local_set(A));
    code.extend(load(
        binary(i32c(PREVIOUS), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(local_set(B));
    code.extend(channels());
    code.extend(local_get(DESTINATION));
    code.extend(local_get(RES));
    code.extend([Op::I32_STORE16, 1, 0]);
    code.extend(local_get(SOURCE_POINTER));
    code.extend(local_set(T3)); // Last selected source, retained across skipped pixels.
    code.push(Op::END);
    for local in [INDEX, SOURCE_POINTER, DESTINATION] {
        code.extend(binary(local_get(local), i32c(2), Op::I32_ADD));
        code.extend(local_set(local));
    }
    code.extend(binary(local_get(INDEX), i32c(64), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);

    // Publish only the last selected pixel's intermediates. No selected pixel
    // means these registers and the weight slot retain their entry values.
    code.extend(binary(
        local_get(MASK),
        binary(reg(abi::ESI), i32c(1), Op::I32_SHR_U),
        Op::I32_SHR_U,
    ));
    code.extend([Op::IF, 0x40]);
    code.extend(store(frame, local_get(T1)));
    code.extend(set_reg(abi::ECX, local_get(T3)));
    for (register, local) in [(abi::EDX, B), (abi::EBX, T2), (abi::EDI, RES)] {
        code.extend(set_reg(register, local_get(local)));
    }
    code.push(Op::END);
    let mut last_eax = local_get(DESTINATION_OFFSET);
    last_eax.extend(local_get(MASK));
    last_eax.extend(binary(local_get(MASK), i32c(i32::MIN), Op::I32_AND));
    last_eax.push(Op::SELECT);
    code.extend(set_reg(abi::EAX, last_eax));
    code.extend(store(source_slot, local_get(SOURCE_POINTER)));
    code.extend(store(bit_slot, i32c(0)));
    code.extend(set_reg(abi::ESI, i32c(64)));
    code.extend(set_reg(abi::FLAGS, i32c(abi::ZF | abi::PF)));
    let mut last_block = i32c(0x498a0b);
    last_block.extend(i32c(0x498a91));
    last_block.extend(binary(local_get(MASK), i32c(i32::MIN), Op::I32_AND));
    last_block.push(Op::SELECT);
    code.extend(set_reg(abi::TRAP_VA, last_block));
    if checkpointed {
        code.extend(set_reg(
            crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL,
            binary(
                reg(crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                binary(local_get(COUNT), i32c(1), Op::I32_SUB),
                Op::I32_SUB,
            ),
        ));
    }
    code.extend(i32c(successor));
    code.extend(local_set(crate::translate::layout::NEXT));
    code.push(Op::BR);
    code.extend(crate::wasm::leb::uleb(depth as u32 + 1));
    code.push(Op::END);
    code
}

fn full_span(successor: i32, depth: i32, checkpointed: bool) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    guard(&mut code, binary(reg(abi::ESI), i32c(64), Op::I32_LT_U));
    guard(
        &mut code,
        binary(reg(abi::ESI), i32c(1), Op::I32_AND)
            .into_iter()
            .chain([Op::I32_EQZ])
            .collect(),
    );
    code.extend(binary(
        binary(i32c(64), reg(abi::ESI), Op::I32_SUB),
        i32c(1),
        Op::I32_SHR_U,
    ));
    code.extend(local_set(COUNT));
    if checkpointed {
        guard(
            &mut code,
            binary(
                local_get(COUNT),
                reg(crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                Op::I32_LE_U,
            ),
        );
    }
    cache_bounds(&mut code);
    guard(&mut code, within(i32c(ALPHA), i64::from(DATA_BYTES)));
    guard(&mut code, binary(reg(abi::EBP), i32c(12), Op::I32_GE_U));
    let frame = binary(reg(abi::EBP), i32c(-12), Op::I32_ADD);
    guard(&mut code, within(frame.clone(), 44));
    guard(
        &mut code,
        disjoint(frame.clone(), i32c(44), i32c(ALPHA), i32c(DATA_BYTES)),
    );
    let source_slot = binary(reg(abi::EBP), i32c(28), Op::I32_ADD);
    code.extend(load(source_slot.clone(), Op::I32_LOAD, 2));
    code.extend(local_set(SOURCE_POINTER));
    code.extend(load(
        binary(reg(abi::EBP), i32c(8), Op::I32_ADD),
        Op::I32_LOAD,
        2,
    ));
    code.extend(local_set(DESTINATION_OFFSET));
    code.extend(binary(
        local_get(SOURCE_POINTER),
        local_get(DESTINATION_OFFSET),
        Op::I32_ADD,
    ));
    code.extend(local_set(DESTINATION));
    let bytes = binary(local_get(COUNT), i32c(2), Op::I32_MUL);
    for pointer in [SOURCE_POINTER, DESTINATION] {
        guard(
            &mut code,
            within_length(local_get(pointer), extend(bytes.clone())),
        );
        guard(
            &mut code,
            disjoint(local_get(pointer), bytes.clone(), frame.clone(), i32c(44)),
        );
    }
    for (address, length) in [
        (i32c(ALPHA), i32c(DATA_BYTES)),
        (local_get(SOURCE_POINTER), bytes.clone()),
    ] {
        guard(
            &mut code,
            disjoint(local_get(DESTINATION), bytes.clone(), address, length),
        );
    }

    code.extend(reg(abi::ESI));
    code.extend(local_set(INDEX));
    code.extend([Op::LOOP, 0x40]);
    code.extend(load(
        binary(i32c(ALPHA), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD8_U,
        0,
    ));
    code.extend(i32c(3));
    code.push(Op::I32_SHR_U);
    code.extend(i32c(5));
    code.push(Op::I32_SHL);
    code.extend(local_set(T1));
    code.extend(load(local_get(SOURCE_POINTER), Op::I32_LOAD16_U, 1));
    code.extend(local_set(A));
    code.extend(load(
        binary(i32c(PREVIOUS), local_get(INDEX), Op::I32_ADD),
        Op::I32_LOAD16_U,
        1,
    ));
    code.extend(local_set(B));
    code.extend(channels());
    code.extend(local_get(DESTINATION));
    code.extend(local_get(RES));
    code.extend([Op::I32_STORE16, 1, 0]);
    for local in [INDEX, SOURCE_POINTER, DESTINATION] {
        code.extend(binary(local_get(local), i32c(2), Op::I32_ADD));
        code.extend(local_set(local));
    }
    code.extend(binary(local_get(INDEX), i32c(64), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);

    code.extend(store(frame, local_get(T1)));
    code.extend(store(source_slot, local_get(SOURCE_POINTER)));
    code.extend(set_reg(abi::EAX, local_get(DESTINATION_OFFSET)));
    code.extend(set_reg(
        abi::ECX,
        binary(local_get(SOURCE_POINTER), i32c(-2), Op::I32_ADD),
    ));
    for (register, local) in [(abi::EDX, B), (abi::EBX, T2), (abi::EDI, RES)] {
        code.extend(set_reg(register, local_get(local)));
    }
    code.extend(set_reg(abi::ESI, i32c(64)));
    code.extend(set_reg(abi::FLAGS, i32c(abi::ZF | abi::PF)));
    if checkpointed {
        code.extend(set_reg(
            crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL,
            binary(
                reg(crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                binary(local_get(COUNT), i32c(1), Op::I32_SUB),
                Op::I32_SUB,
            ),
        ));
    }
    code.extend(i32c(successor));
    code.extend(local_set(crate::translate::layout::NEXT));
    code.push(Op::BR);
    code.extend(crate::wasm::leb::uleb(depth as u32 + 1));
    code.push(Op::END);
    code
}

#[cfg(test)]
#[path = "../../../../tests/unit/translate/kernels/compositor_alpha.rs"]
mod tests;
