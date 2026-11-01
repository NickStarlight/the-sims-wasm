//! Cooperative checkpoints independent of compatibility API dispatch.
use super::{
    abi,
    emit_helpers::{i32c, set_reg, Seq},
};
use crate::wasm::{leb::sleb, opcodes::Op};

pub(super) const STRAIGHT_LINE_INTERVAL: usize = 256;
pub(super) const REP_INTERVAL: i32 = 1024;

/// The host may suspend this plain `(i32) -> ()` import without changing guest state.
/// `blockTrace` is retained as the linking name for ABI compatibility.
pub(super) fn checkpoint(import: Option<u32>, guest_va: u32) -> Seq {
    let mut code = set_reg(abi::TRAP_VA, i32c(guest_va as i32));
    if let Some(index) = import {
        code.extend(i32c(guest_va as i32));
        code.push(Op::CALL);
        code.extend(sleb(index as i32));
    }
    code
}

#[cfg(test)]
#[path = "../../tests/unit/translate/scheduling.rs"]
mod tests;

// Imported guest registers occupy the preceding globals. Each Wasm instance
// owns this counter, so cooperative guest threads never share a polling budget.
pub(super) const CHECKPOINT_BUDGET_GLOBAL: u8 = 16;
pub(super) const BLOCK_INTERVAL: i32 = 256;

/// Hot basic blocks poll entirely in Wasm; only an exhausted budget calls JSPI.
pub(super) fn block_checkpoint(import: Option<u32>, guest_va: u32) -> Seq {
    let mut code = set_reg(abi::TRAP_VA, i32c(guest_va as i32));
    if let Some(index) = import {
        code.extend(set_reg(CHECKPOINT_BUDGET_GLOBAL, {
            let mut value = super::emit_helpers::reg(CHECKPOINT_BUDGET_GLOBAL);
            value.extend(i32c(1));
            value.push(Op::I32_SUB);
            value
        }));
        code.extend(super::emit_helpers::reg(CHECKPOINT_BUDGET_GLOBAL));
        code.push(Op::I32_EQZ);
        code.extend([Op::IF, 0x40]);
        code.extend(set_reg(CHECKPOINT_BUDGET_GLOBAL, i32c(BLOCK_INTERVAL)));
        code.extend(i32c(guest_va as i32));
        code.push(Op::CALL);
        code.extend(sleb(index as i32));
        code.push(Op::END);
    }
    code
}
