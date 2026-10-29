//! Batch a verified signed-byte string hash without crossing a checkpoint.
//! The complete native routine is fingerprinted by the parent recognizer.
use super::*;
use crate::translate::layout::NEXT;

const POINTER: u32 = 4;
const HASH: u32 = 5;
const CHARACTER: u32 = 6;
const SHIFTED: u32 = 7;
const NEXT_BYTE: u32 = 8;
const COUNT: u32 = 12;

pub(super) fn emit(
    address: u32,
    context: &crate::translate::emit::EmitCtx,
    depth: i32,
) -> Option<Seq> {
    if address != 0x5b8dc7 {
        return None;
    }
    let repeat = *context.id.get(&0x5b8dc7)?;
    let finish = *context.id.get(&0x5b8de1)?;
    let checkpointed = context.block_trace.is_some();
    let budget = crate::translate::scheduling::CHECKPOINT_BUDGET_GLOBAL;
    let mut code = vec![Op::BLOCK, 0x40];
    code.extend(i32c(64));
    if checkpointed {
        code.extend(reg(budget));
        code.extend(binary(reg(budget), i32c(64), Op::I32_GE_U));
        code.push(Op::SELECT);
    }
    code.extend(local_set(T1));
    code.extend(binary(local_get(T1), i32c(2), Op::I32_LT_U));
    code.extend([Op::BR_IF, 0]);
    // Validate the whole potential read span before mutating guest state. A
    // boundary case retains the scalar path and its partial-fault registers.
    code.extend(within_length(
        reg(abi::EDX),
        extend(binary(local_get(T1), i32c(1), Op::I32_ADD)),
    ));
    code.extend([Op::I32_EQZ, Op::BR_IF, 0]);
    for (slot, value) in [
        (POINTER, reg(abi::EDX)),
        (HASH, reg(abi::EAX)),
        (CHARACTER, reg(abi::ECX)),
        (COUNT, i32c(0)),
    ] {
        code.extend(value);
        code.extend(local_set(slot));
    }
    code.extend([Op::LOOP, 0x40]);
    // MOVSX ECX,CL, including bytes >=0x80. CL is then replaced with the
    // following byte while the other 24 bits retain that sign extension.
    code.extend(sign_byte(local_get(CHARACTER)));
    code.extend(local_set(CHARACTER));
    code.extend(binary(local_get(HASH), local_get(CHARACTER), Op::I32_ADD));
    code.extend(i32c(0x401));
    code.push(Op::I32_MUL);
    code.extend(local_set(HASH));
    code.extend(load(
        binary(local_get(POINTER), i32c(1), Op::I32_ADD),
        Op::I32_LOAD8_U,
        0,
    ));
    code.extend(local_set(NEXT_BYTE));
    code.extend(binary(local_get(HASH), i32c(6), Op::I32_SHR_U));
    code.extend(local_set(SHIFTED));
    code.extend(binary(local_get(HASH), local_get(SHIFTED), Op::I32_XOR));
    code.extend(local_set(HASH));
    code.extend(binary(local_get(POINTER), i32c(1), Op::I32_ADD));
    code.extend(local_set(POINTER));
    code.extend(binary(
        binary(local_get(CHARACTER), i32c(-256), Op::I32_AND),
        local_get(NEXT_BYTE),
        Op::I32_OR,
    ));
    code.extend(local_set(CHARACTER));
    code.extend(binary(local_get(COUNT), i32c(1), Op::I32_ADD));
    code.extend(local_set(COUNT));
    code.extend(binary(local_get(COUNT), local_get(T1), Op::I32_LT_U));
    code.extend(local_get(NEXT_BYTE));
    code.push(Op::I32_EQZ);
    code.push(Op::I32_EQZ);
    code.extend([Op::I32_AND, Op::BR_IF, 0, Op::END]);
    for (register, slot) in [
        (abi::EAX, HASH),
        (abi::ECX, CHARACTER),
        (abi::EDX, POINTER),
        (abi::ESI, SHIFTED),
    ] {
        code.extend(set_reg(register, local_get(slot)));
    }
    // The preceding full-width XOR clears unrelated flags. Sign-extending the
    // final byte makes the ordinary logical-flags helper match TEST CL,CL.
    code.extend(sign_byte(local_get(NEXT_BYTE)));
    code.extend(local_set(RES));
    code.extend(flag_logic());
    if checkpointed {
        // The enclosing block consumed the first tick. Leave the next actual
        // poll to the original dispatcher, with the exact architectural state.
        code.extend(set_reg(
            budget,
            binary(
                reg(budget),
                binary(local_get(COUNT), i32c(1), Op::I32_SUB),
                Op::I32_SUB,
            ),
        ));
    }
    code.extend(i32c(repeat));
    code.extend(i32c(finish));
    code.extend(local_get(NEXT_BYTE));
    code.push(Op::SELECT);
    code.extend(local_set(NEXT));
    code.push(Op::BR);
    code.extend(crate::wasm::leb::uleb(depth as u32 + 1));
    code.push(Op::END);
    Some(code)
}

fn sign_byte(value: Seq) -> Seq {
    binary(
        binary(value, i32c(24), Op::I32_SHL),
        i32c(24),
        Op::I32_SHR_S,
    )
}
