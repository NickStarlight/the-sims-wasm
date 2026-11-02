//! Batch the fingerprinted row's alpha classification without crossing a poll.
use super::*;

pub(super) fn emit(successor: i32, depth: i32, checkpointed: bool) -> Seq {
    let mut code = vec![Op::BLOCK, 0x40];
    let guard = |code: &mut Seq, condition: Seq| {
        code.extend(condition);
        code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    };
    guard(&mut code, binary(reg(abi::ECX), i32c(32), Op::I32_LT_U));
    guard(
        &mut code,
        binary(
            reg(abi::EDX),
            binary(
                i32c(ALPHA),
                binary(reg(abi::ECX), i32c(2), Op::I32_MUL),
                Op::I32_ADD,
            ),
            Op::I32_EQ,
        ),
    );
    guard(
        &mut code,
        binary(
            reg(abi::ESI),
            binary(i32c(1), reg(abi::ECX), Op::I32_SHL),
            Op::I32_EQ,
        ),
    );
    let remaining = binary(i32c(32), reg(abi::ECX), Op::I32_SUB);
    if checkpointed {
        // A pixel visits 3 or 4 blocks; the opaque block includes its update.
        // Reserve the worst case before touching guest state, then charge the
        // exact path count after the nonfaulting span.
        guard(
            &mut code,
            binary(
                binary(remaining.clone(), i32c(4), Op::I32_MUL),
                reg(crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL),
                Op::I32_LE_U,
            ),
        );
    }
    cache_bounds(&mut code);
    guard(&mut code, within(i32c(ALPHA), 64));
    guard(&mut code, binary(reg(abi::EBP), i32c(16), Op::I32_GE_U));
    let frame = binary(reg(abi::EBP), i32c(-16), Op::I32_ADD);
    guard(&mut code, within(frame.clone(), 52));
    guard(
        &mut code,
        disjoint(frame.clone(), i32c(52), i32c(ALPHA), i32c(64)),
    );
    let slot = |offset| binary(reg(abi::EBP), i32c(offset), Op::I32_ADD);
    code.extend(binary(
        load(slot(16), Op::I32_LOAD, 2),
        reg(abi::ECX),
        Op::I32_ADD,
    ));
    code.extend(local_set(SOURCE_POINTER));
    guard(
        &mut code,
        within_length(local_get(SOURCE_POINTER), extend(remaining.clone())),
    );
    // The original loop reloads the source pointer/offset and updates masks each
    // iteration. Defer those writes only when neither source nor alpha aliases them.
    for (address, length) in [(frame, i32c(52)), (i32c(ALPHA), i32c(64))] {
        guard(
            &mut code,
            disjoint(
                local_get(SOURCE_POINTER),
                remaining.clone(),
                address,
                length,
            ),
        );
    }
    for (offset, local) in [(32, T1), (28, MASK), (-16, T2)] {
        code.extend(load(slot(offset), Op::I32_LOAD, 2));
        code.extend(local_set(local));
    }
    for (register, local) in [(abi::ECX, INDEX), (abi::EDX, DESTINATION), (abi::ESI, B)] {
        code.extend(reg(register));
        code.extend(local_set(local));
    }
    code.extend(i32c(0));
    code.extend(local_set(COUNT));
    code.extend([Op::LOOP, 0x40]);
    code.extend(binary(
        load(local_get(SOURCE_POINTER), Op::I32_LOAD8_U, 0),
        local_get(T1),
        Op::I32_ADD,
    ));
    code.extend(local_set(A));
    code.extend(binary(local_get(COUNT), i32c(3), Op::I32_ADD));
    code.extend(local_set(COUNT));
    code.extend(binary(local_get(A), i32c(7), Op::I32_GT_S));
    code.extend([Op::IF, 0x40]);
    code.extend(binary(local_get(COUNT), i32c(1), Op::I32_ADD));
    code.extend(local_set(COUNT));
    code.extend(binary(local_get(A), i32c(248), Op::I32_LT_S));
    code.extend([Op::IF, 0x40]);
    code.extend(binary(local_get(MASK), local_get(B), Op::I32_OR));
    code.extend(local_set(MASK));
    code.push(Op::ELSE);
    code.extend(binary(local_get(T2), local_get(B), Op::I32_OR));
    code.extend(local_set(T2));
    code.extend([Op::END, Op::END]);
    code.extend(local_get(DESTINATION));
    code.extend(local_get(A));
    code.extend([Op::I32_STORE16, 1, 0]);
    for (local, step) in [(SOURCE_POINTER, 1), (DESTINATION, 2), (INDEX, 1)] {
        code.extend(binary(local_get(local), i32c(step), Op::I32_ADD));
        code.extend(local_set(local));
    }
    code.extend(binary(local_get(B), i32c(1), Op::I32_SHL));
    code.extend(local_set(B));
    code.extend(binary(local_get(INDEX), i32c(32), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0, Op::END]);
    code.extend(store(slot(28), local_get(MASK)));
    code.extend(store(slot(-16), local_get(T2)));
    code.extend(set_reg(abi::EAX, local_get(A)));
    code.extend(set_reg(abi::ECX, i32c(32)));
    code.extend(set_reg(abi::EDX, i32c(ALPHA + 64)));
    code.extend(set_reg(abi::ESI, i32c(0)));
    code.extend(set_reg(abi::FLAGS, i32c(abi::ZF | abi::PF)));
    let mut last_block = i32c(0x4988e8);
    last_block.extend(i32c(0x4988eb));
    last_block.extend(binary(local_get(A), i32c(247), Op::I32_GT_S));
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

#[cfg(test)]
#[path = "../../../../../tests/unit/translate/kernels/compositor_classify.rs"]
mod tests;
