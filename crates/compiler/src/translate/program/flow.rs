//! Direct forward edges between ascending segment bodies. Backedges retain the
//! resumable dispatcher; every destination still executes its original checkpoint.
use crate::translate::{
    abi,
    cond::emit_cond,
    emit::EmitCtx,
    emit_helpers::{i32c, local_set, reg, Seq},
    layout::NEXT,
};
use crate::wasm::{leb::uleb, opcodes::Op};
use crate::x86::types::{Insn, Mnem};

pub(super) struct ForwardFlow {
    current: i32,
    count: i32,
}

impl ForwardFlow {
    pub(super) fn new(current: usize, count: usize) -> Self {
        debug_assert!(current < count);
        Self {
            current: current as i32,
            count: count as i32,
        }
    }

    pub(super) fn loop_depth(&self) -> u32 {
        (self.count - self.current - 1) as u32
    }

    fn forward_depth(&self, target: i32) -> Option<u32> {
        (target > self.current && target < self.count).then(|| (target - self.current - 1) as u32)
    }

    fn branch(&self, target: i32, nesting: u32) -> Seq {
        let mut code = Vec::new();
        let depth = match self.forward_depth(target) {
            Some(depth) => depth,
            None => {
                code.extend(i32c(target));
                code.extend(local_set(NEXT));
                self.loop_depth()
            }
        };
        code.push(Op::BR);
        code.extend(uleb(depth + nesting));
        code
    }

    pub(super) fn transfer(&self, target: i32) -> Seq {
        if self.forward_depth(target) == Some(0) {
            // The next END closes the destination's entry label.
            Vec::new()
        } else {
            self.branch(target, 0)
        }
    }

    fn conditional(&self, condition: Seq, target: i32, successor: i32) -> Seq {
        let mut code = condition;
        if let Some(depth) = self.forward_depth(target) {
            code.push(Op::BR_IF);
            code.extend(uleb(depth));
        } else {
            code.extend([Op::IF, 0x40]);
            code.extend(self.branch(target, 1));
            code.push(Op::END);
        }
        code.extend(self.transfer(successor));
        code
    }

    /// Leave calls, returns, external transfers and indirect switches with the
    /// existing emitter, which receives loop_depth() for its dispatch branches.
    pub(super) fn instruction(
        &self,
        insn: &Insn,
        context: &EmitCtx,
        successor: i32,
    ) -> Option<Seq> {
        match insn.mnem {
            Mnem::Jmp => insn
                .rel
                .and_then(|address| context.id.get(&address))
                .map(|&target| self.transfer(target)),
            Mnem::Jcc | Mnem::Jcxz => {
                let condition = if insn.mnem == Mnem::Jcxz {
                    let mut code = reg(abi::ECX);
                    code.push(Op::I32_EQZ);
                    code
                } else {
                    emit_cond(insn.cc.unwrap_or(0))
                };
                // Match emit_insn's existing unresolved-conditional semantics.
                let target = insn
                    .rel
                    .and_then(|address| context.id.get(&address).copied())
                    .unwrap_or(successor);
                Some(self.conditional(condition, target, successor))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/program/flow.rs"]
mod tests;
