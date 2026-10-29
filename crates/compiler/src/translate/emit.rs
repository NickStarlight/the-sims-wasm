//! Per-instruction translation: x86 → wasm code sequences.

use super::abi;
use super::cond::emit_cond;
use super::emit_flags::*;
use super::emit_fpu::emit_fpu;
use super::emit_helpers::{
    emit_address, emit_load, emit_store, i32c, i64c, reg, set_reg, size_mask, Seq,
};
use super::emit_helpers::{local_get, local_set};
use super::error::EmitError;
use super::layout::{A, B, ENTRY_SP, FULL, NEXT, RES, T1, T2, T3};
use crate::wasm::leb::sleb;
use crate::wasm::opcodes::Op;
use crate::x86::types::{Insn, MemOp, Operand, Reg};

/// Function-local bindings used during instruction emission.
pub struct EmitCtx<'a> {
    /// segment-start VA → dispatch id (per function)
    pub id: std::collections::BTreeMap<u32, i32>,
    /// sorted function VA → wasm func idx (guest callee)
    pub func_idx_of: &'a std::collections::HashMap<u32, i32>,
    /// wasm function index of the $idxOf helper
    pub idx_of: i32,
    /// IAT slot VA → shim table index
    pub iat_shim: &'a std::collections::HashMap<u32, i32>,
    /// Imported cooperative checkpoint, linked as env.blockTrace (index 1).
    pub block_trace: Option<u32>,
}

const ESP: u8 = abi::ESP;

/// A narrow peephole: both instructions must be nonfaulting 32-bit ALU
/// operations and the second must overwrite every flag produced by the first.
/// The caller must not cross a checkpoint or segment boundary.
pub(crate) fn emit_overwritten_flags(insn: &Insn, next: &Insn) -> Option<Seq> {
    use crate::x86::types::Mnem::*;
    let pure_alu = |i: &Insn| {
        matches!(i.mnem, Add | Sub | And | Or | Xor | Cmp | Test)
            && matches!(i.ops.first(), Some(Operand::Reg { size: 4, .. }))
            && matches!(
                i.ops.get(1),
                Some(Operand::Reg { size: 4, .. } | Operand::Imm(_))
            )
    };
    if !pure_alu(insn) || !pure_alu(next) {
        return None;
    }
    if matches!(insn.mnem, Cmp | Test) {
        return Some(Vec::new());
    }
    let op = match insn.mnem {
        Add => Op::I32_ADD,
        Sub => Op::I32_SUB,
        And => Op::I32_AND,
        Or => Op::I32_OR,
        Xor => Op::I32_XOR,
        _ => unreachable!(),
    };
    let mut value = read_op(&insn.ops[0]);
    value.extend(read_op(&insn.ops[1]));
    value.push(op);
    Some(write_op(&insn.ops[0], value))
}

/// Emit one instruction. Sets NEXT=succId on fallthrough; returns for terminators.
pub fn emit_insn(
    insn: &Insn,
    ctx: &EmitCtx,
    succ_id: i32,
    br_depth: i32,
) -> Result<Seq, EmitError> {
    let mut out: Seq = Vec::new();
    let a = insn.ops.first();
    let b = insn.ops.get(1);

    match insn.mnem {
        crate::x86::types::Mnem::Nop => {}
        crate::x86::types::Mnem::Fpu => out.extend(emit_fpu(insn)?),
        crate::x86::types::Mnem::Pushad => {
            let orig_esp = {
                let mut v = reg(ESP);
                v.extend(local_set(T3));
                v
            };
            out.extend(orig_esp);
            for r in [abi::EAX, abi::ECX, abi::EDX, abi::EBX] {
                let mut esp_upd = reg(ESP);
                esp_upd.extend(i32c(-4));
                esp_upd.push(Op::I32_ADD);
                out.extend(set_reg(ESP, esp_upd));
                out.extend(emit_store(&mem_at(ESP, 0, 4), reg(r)));
            }
            let mut esp_upd = reg(ESP);
            esp_upd.extend(i32c(-4));
            esp_upd.push(Op::I32_ADD);
            out.extend(set_reg(ESP, esp_upd));
            out.extend(emit_store(&mem_at(ESP, 0, 4), local_get(T3)));
            for r in [abi::EBP, abi::ESI, abi::EDI] {
                let mut esp_upd = reg(ESP);
                esp_upd.extend(i32c(-4));
                esp_upd.push(Op::I32_ADD);
                out.extend(set_reg(ESP, esp_upd));
                out.extend(emit_store(&mem_at(ESP, 0, 4), reg(r)));
            }
        }
        crate::x86::types::Mnem::Popad => {
            for r in [abi::EDI, abi::ESI, abi::EBP] {
                out.extend(write_op(
                    &Operand::Reg { r, size: 4 },
                    emit_load(&mem_at(ESP, 0, 4)),
                ));
                let mut esp_upd = reg(ESP);
                esp_upd.extend(i32c(4));
                esp_upd.push(Op::I32_ADD);
                out.extend(set_reg(ESP, esp_upd));
            }
            // skip pushed esp
            let mut esp_upd = reg(ESP);
            esp_upd.extend(i32c(4));
            esp_upd.push(Op::I32_ADD);
            out.extend(set_reg(ESP, esp_upd));
            for r in [abi::EBX, abi::EDX, abi::ECX, abi::EAX] {
                out.extend(write_op(
                    &Operand::Reg { r, size: 4 },
                    emit_load(&mem_at(ESP, 0, 4)),
                ));
                let mut esp_upd = reg(ESP);
                esp_upd.extend(i32c(4));
                esp_upd.push(Op::I32_ADD);
                out.extend(set_reg(ESP, esp_upd));
            }
        }
        crate::x86::types::Mnem::Mov => out.extend(write_op(a.unwrap(), read_op(b.unwrap()))),
        crate::x86::types::Mnem::Xchg => {
            // decoder yields reg/reg, no mem form (XCHG eAX,r path)
            out.extend(stash(b.unwrap(), T1));
            out.extend(write_op(b.unwrap(), read_op(a.unwrap())));
            out.extend(write_op(a.unwrap(), local_get(T1)));
        }
        crate::x86::types::Mnem::Lea => {
            let m = match b {
                Some(Operand::Mem(m)) => m.clone(),
                _ => return Err(EmitError::new(insn.va, "LEA requires a memory operand")),
            };
            out.extend(write_op(a.unwrap(), emit_address(&m)));
        }
        crate::x86::types::Mnem::Push => out.extend(stack::emit_push(insn)),
        crate::x86::types::Mnem::Pop => out.extend(stack::emit_pop(insn)),
        crate::x86::types::Mnem::Call => {
            let fake_ret = insn.va + insn.size;
            out.extend(set_reg(abi::ACTUAL_RETURN, i32c(-1)));
            if let Some(rel) = insn.rel {
                let fi = ctx.func_idx_of.get(&rel).copied().unwrap_or(-1);
                if fi >= 0 {
                    out.extend(call_direct(fi, Some(fake_ret)));
                } else {
                    out.extend(call_indirect(
                        &Operand::Imm(rel as i32),
                        ctx,
                        Some(fake_ret),
                    ));
                }
            } else {
                out.extend(call_indirect(a.unwrap(), ctx, Some(fake_ret)));
            }
            // Falling out of a callee without a native RET is never a valid
            // return (zero itself is a valid top-level return sentinel).
            out.extend(reg(abi::ACTUAL_RETURN));
            out.extend(i32c(-1));
            out.push(Op::I32_EQ);
            out.push(Op::IF);
            out.push(0x40);
            out.extend(set_reg(abi::TRAP_VA, i32c(insn.va as i32)));
            out.push(Op::UNREACHABLE);
            out.push(Op::END);
            // Native SEH and longjmp can return through a different guest frame.
            // Preserve the returned CPU state while discarding stale Wasm frames.
            // The stack bound disambiguates recursive calls at the same x86 VA.
            out.extend(reg(abi::ACTUAL_RETURN));
            out.extend(i32c(fake_ret as i32));
            out.push(Op::I32_NE);
            out.extend(reg(ESP));
            out.extend(local_get(ENTRY_SP));
            out.push(Op::I32_GT_U);
            out.push(Op::I32_OR);
            out.push(Op::IF);
            out.push(0x40);
            out.push(Op::RETURN);
            out.push(Op::END);
            out.extend(set_reg(abi::ACTUAL_RETURN, i32c(-1)));
        }
        crate::x86::types::Mnem::Jmp => {
            if let Some(rel) = insn.rel {
                let target = ctx.id.get(&rel).copied();
                let fi = ctx.func_idx_of.get(&rel).copied().unwrap_or(-1);
                if let Some(target) = target {
                    out.extend(to(target, br_depth));
                } else {
                    if fi >= 0 {
                        out.extend(call_direct(fi, None)); // no fakeRet (tail transfer)
                    } else {
                        out.extend(call_indirect(&Operand::Imm(rel as i32), ctx, None));
                    }
                    out.push(Op::RETURN);
                }
            } else {
                let entries = &insn.switch_targets;
                if !entries.is_empty() {
                    let inner = br_depth + 1; // one extra if-frame for the match guard
                    out.extend(stash(a.unwrap(), T1));
                    for e in entries {
                        let id = ctx.id.get(e).copied();
                        if id.is_none() {
                            continue;
                        }
                        let mut piece = local_get(T1);
                        piece.extend(i32c(*e as i32));
                        piece.push(Op::I32_EQ);
                        piece.push(Op::IF);
                        piece.push(0x40);
                        piece.extend(to(id.unwrap(), inner));
                        piece.push(Op::END);
                        out.extend(piece);
                    }
                    out.push(Op::UNREACHABLE);
                } else {
                    out.extend(call_indirect(a.unwrap(), ctx, None));
                    out.push(Op::RETURN);
                }
            }
        }
        crate::x86::types::Mnem::Ret => {
            out.extend(set_reg(abi::ACTUAL_RETURN, emit_load(&mem_at(ESP, 0, 4))));
            let n = match a {
                Some(Operand::Imm(v)) => *v,
                _ => 0,
            };
            let mut esp_upd = reg(ESP);
            esp_upd.extend(i32c(4 + n));
            esp_upd.push(Op::I32_ADD);
            out.extend(set_reg(ESP, esp_upd));
            out.push(Op::RETURN);
        }
        crate::x86::types::Mnem::Leave => {
            out.extend(set_reg(ESP, reg(abi::EBP)));
            out.extend(set_reg(abi::EBP, emit_load(&mem_at(ESP, 0, 4))));
            let mut esp_upd = reg(ESP);
            esp_upd.extend(i32c(4));
            esp_upd.push(Op::I32_ADD);
            out.extend(set_reg(ESP, esp_upd));
        }
        crate::x86::types::Mnem::Jcc | crate::x86::types::Mnem::Jcxz => {
            let cond = if insn.mnem == crate::x86::types::Mnem::Jcxz {
                let mut v = reg(abi::ECX);
                v.push(Op::I32_EQZ);
                v
            } else {
                emit_cond(insn.cc.unwrap_or(0))
            };
            let t_id = insn
                .rel
                .and_then(|rel| ctx.id.get(&rel).copied())
                .unwrap_or(succ_id);
            out.extend(i32c(t_id));
            out.extend(i32c(succ_id));
            out.extend(cond);
            out.push(Op::SELECT);
            out.extend(local_set(NEXT));
            out.push(Op::BR);
            out.extend(sleb(br_depth));
        }
        crate::x86::types::Mnem::Setcc => {
            let mut v = emit_cond(insn.cc.unwrap_or(0));
            v.extend(i32c(1));
            v.push(Op::I32_AND);
            out.extend(write_op(a.unwrap(), v));
        }
        _ => out.extend(emit_alu(insn, ctx)?),
    }
    Ok(out)
}

/// synthetic helpers via the dispatch import (index 0)
fn helper_call(k: i32) -> Seq {
    let mut v = i32c(k);
    v.push(Op::CALL);
    v.push(0);
    v
}

mod alu;
mod arithmetic;
mod bits;
mod calls;
mod exchange;
mod operands;
mod stack;
mod strings;
use alu::emit_alu;
use arithmetic::{
    binop, carry_in, emit_div, emit_imul, emit_mul_long, emit_shift, sign_extend, unop,
};
use bits::{emit_bs, emit_bswap, emit_bt};
use calls::{call_direct, call_indirect};
use exchange::{emit_cmpxchg, emit_xadd, emit_xchg};
use operands::{mem_at, pick, read_op, stash, to, write_op};
use strings::emit_string;
