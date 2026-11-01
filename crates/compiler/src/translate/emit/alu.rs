//! Dispatch integer, bit, and string instructions.

use super::{
    abi, binop, carry_in, emit_bs, emit_bswap, emit_bt, emit_cmpxchg, emit_div, emit_imul,
    emit_mul_long, emit_shift, emit_string, emit_xadd, emit_xchg, flag_add, flag_add_carry,
    flag_inc_dec, flag_logic, flag_narrow_alu, flag_neg, flag_sh_double, flag_sub, flag_sub_borrow,
    helper_call, i32c, local_get, local_set, read_op, reg, set_reg, sign_extend, stash, unop,
    write_op, EmitCtx, Insn, Op, Operand, Seq, A, B, RES, T1,
};
use crate::translate::error::EmitError;

/// Everything with operands: ALU, shifts, bit ops, string ops, misc.
pub(super) fn emit_alu(insn: &Insn, ctx: &EmitCtx) -> Result<Seq, EmitError> {
    let mut out: Seq = Vec::new();
    let a = insn.ops.first();
    let b = insn.ops.get(1);

    match insn.mnem {
        crate::x86::types::Mnem::Add => {
            out.extend(binop(insn, Some(Op::I32_ADD), flag_add(), true, None))
        }
        crate::x86::types::Mnem::Sub => {
            out.extend(binop(insn, Some(Op::I32_SUB), flag_sub(), true, None))
        }
        crate::x86::types::Mnem::And => {
            out.extend(binop(insn, Some(Op::I32_AND), flag_logic(), true, None))
        }
        crate::x86::types::Mnem::Or => {
            out.extend(binop(insn, Some(Op::I32_OR), flag_logic(), true, None))
        }
        crate::x86::types::Mnem::Xor => {
            out.extend(binop(insn, Some(Op::I32_XOR), flag_logic(), true, None))
        }
        crate::x86::types::Mnem::Cmp => {
            out.extend(binop(insn, Some(Op::I32_SUB), flag_sub(), false, None))
        }
        crate::x86::types::Mnem::Test => {
            out.extend(binop(insn, Some(Op::I32_AND), flag_logic(), false, None))
        }
        crate::x86::types::Mnem::Adc => out.extend(binop(
            insn,
            None,
            flag_add_carry(),
            true,
            Some(|mut x, y| {
                x.extend(y);
                x.extend(carry_in());
                x.push(Op::I32_ADD);
                x.push(Op::I32_ADD);
                x
            }),
        )),
        crate::x86::types::Mnem::Sbb => out.extend(binop(
            insn,
            None,
            flag_sub_borrow(),
            true,
            Some(|mut x, y| {
                x.extend(y);
                x.push(Op::I32_SUB);
                x.extend(carry_in());
                x.push(Op::I32_SUB);
                x
            }),
        )),
        crate::x86::types::Mnem::Inc => out.extend(unop(insn, true, flag_inc_dec(true))),
        crate::x86::types::Mnem::Dec => out.extend(unop(insn, false, flag_inc_dec(false))),
        crate::x86::types::Mnem::Neg => {
            out.extend(stash(a.unwrap(), A));
            let size = match a.unwrap() {
                Operand::Reg { size, .. } => *size,
                Operand::Mem(m) => m.size,
                _ => 4,
            };
            if size == 1 || size == 2 {
                out.extend(local_get(A));
                out.extend(local_set(B));
                out.extend(i32c(0));
                out.extend(local_set(A));
                out.extend(i32c(0));
                out.extend(local_get(B));
                out.push(Op::I32_SUB);
                out.extend(local_set(RES));
                out.extend(flag_narrow_alu(insn.mnem, size));
            } else {
                out.extend(flag_neg());
            }
            out.extend(write_op(a.unwrap(), local_get(RES)));
        }
        crate::x86::types::Mnem::Not => {
            let mut v = read_op(a.unwrap());
            v.extend(i32c(-1));
            v.push(Op::I32_XOR);
            out.extend(write_op(a.unwrap(), v));
        }
        crate::x86::types::Mnem::Mul => out.extend(emit_mul_long(false, a.unwrap())),
        crate::x86::types::Mnem::Imul => out.extend(emit_imul(insn)),
        crate::x86::types::Mnem::Div => out.extend(emit_div(false, a.unwrap())),
        crate::x86::types::Mnem::Idiv => out.extend(emit_div(true, a.unwrap())),
        crate::x86::types::Mnem::Shl
        | crate::x86::types::Mnem::Shr
        | crate::x86::types::Mnem::Sar
        | crate::x86::types::Mnem::Rol
        | crate::x86::types::Mnem::Ror
        | crate::x86::types::Mnem::Rcl
        | crate::x86::types::Mnem::Rcr => out.extend(emit_shift(insn)),
        crate::x86::types::Mnem::Shld | crate::x86::types::Mnem::Shrd => {
            let left = insn.mnem == crate::x86::types::Mnem::Shld;
            out.extend(stash(a.unwrap(), A));
            out.extend(stash(b.unwrap(), B));
            out.extend(stash(insn.ops.get(2).unwrap(), T1));
            out.extend(local_get(T1));
            out.extend(i32c(31));
            out.push(Op::I32_AND);
            out.push(Op::IF);
            out.push(0x40);
            if left {
                out.extend(local_get(A));
                out.extend(local_get(T1));
                out.extend(i32c(31));
                out.push(Op::I32_AND);
                out.push(Op::I32_SHL);
                out.extend(local_get(B));
                out.extend(i32c(32));
                out.extend(local_get(T1));
                out.extend(i32c(31));
                out.push(Op::I32_AND);
                out.push(Op::I32_SUB);
                out.push(Op::I32_SHR_U);
                out.push(Op::I32_OR);
            } else {
                out.extend(local_get(A));
                out.extend(local_get(T1));
                out.extend(i32c(31));
                out.push(Op::I32_AND);
                out.push(Op::I32_SHR_U);
                out.extend(local_get(B));
                out.extend(i32c(32));
                out.extend(local_get(T1));
                out.extend(i32c(31));
                out.push(Op::I32_AND);
                out.push(Op::I32_SUB);
                out.push(Op::I32_SHL);
                out.push(Op::I32_OR);
            }
            out.extend(local_set(RES));
            out.extend(write_op(a.unwrap(), local_get(RES)));
            out.extend(flag_sh_double(left));
            out.push(Op::END);
        }
        crate::x86::types::Mnem::Bt => {
            out.extend(emit_bt(a.unwrap(), b.unwrap(), false, Op::I32_OR, false))
        }
        crate::x86::types::Mnem::Bts => {
            out.extend(emit_bt(a.unwrap(), b.unwrap(), true, Op::I32_OR, false))
        }
        crate::x86::types::Mnem::Btr => {
            out.extend(emit_bt(a.unwrap(), b.unwrap(), true, Op::I32_AND, true))
        }
        crate::x86::types::Mnem::Btc => {
            out.extend(emit_bt(a.unwrap(), b.unwrap(), true, Op::I32_XOR, false))
        }
        crate::x86::types::Mnem::Bsf => out.extend(emit_bs(true, a.unwrap(), b.unwrap())),
        crate::x86::types::Mnem::Bsr => out.extend(emit_bs(false, a.unwrap(), b.unwrap())),
        crate::x86::types::Mnem::Bswap => out.extend(emit_bswap(a.unwrap())),
        crate::x86::types::Mnem::Xadd => out.extend(emit_xadd(a.unwrap(), b.unwrap())),
        crate::x86::types::Mnem::Cmpxchg => out.extend(emit_cmpxchg(a.unwrap(), b.unwrap())),
        crate::x86::types::Mnem::Movsx => {
            let size = match b.unwrap() {
                Operand::Reg { size, .. } => *size,
                Operand::Mem(m) => m.size,
                _ => 2,
            };
            out.extend(write_op(a.unwrap(), sign_extend(read_op(b.unwrap()), size)));
        }
        crate::x86::types::Mnem::Movzx => out.extend(write_op(a.unwrap(), read_op(b.unwrap()))),
        crate::x86::types::Mnem::Xchg => out.extend(emit_xchg(a.unwrap(), b.unwrap())),
        crate::x86::types::Mnem::Movs
        | crate::x86::types::Mnem::Stos
        | crate::x86::types::Mnem::Lods
        | crate::x86::types::Mnem::Scas
        | crate::x86::types::Mnem::Cmps => out.extend(emit_string(insn, ctx.block_trace)),
        crate::x86::types::Mnem::Xlat => {
            let mut address = reg(abi::EBX);
            address.extend(reg(abi::EAX));
            address.extend(i32c(0xff));
            address.push(Op::I32_AND);
            address.push(Op::I32_ADD); // i32 addition wraps before the memory access
            if matches!(b, Some(Operand::Imm(16))) {
                address.extend(i32c(0xffff));
                address.push(Op::I32_AND);
            }
            if matches!(a, Some(Operand::Mem(m)) if m.fs) {
                address.extend(reg(abi::TEB_BASE));
                address.push(Op::I32_ADD);
            }
            address.extend([Op::I32_LOAD8_U, 0, 0]);
            out.extend(write_op(
                &Operand::Reg {
                    r: abi::EAX,
                    size: 1,
                },
                address,
            ));
        }
        crate::x86::types::Mnem::Pushf => out.extend(super::stack::emit_push_flags(insn)),
        crate::x86::types::Mnem::Popf => out.extend(super::stack::emit_pop_flags(insn)),
        crate::x86::types::Mnem::Clc => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(!abi::CF));
            v.push(Op::I32_AND);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Stc => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(abi::CF));
            v.push(Op::I32_OR);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Cmc => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(abi::CF));
            v.push(Op::I32_XOR);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Cld => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(!abi::DF));
            v.push(Op::I32_AND);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Std => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(abi::DF));
            v.push(Op::I32_OR);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Cwde => {
            out.extend(set_reg(abi::EAX, sign_extend(reg(abi::EAX), 2)))
        }
        crate::x86::types::Mnem::Cdq => {
            // Arithmetic shift already yields the required all-ones sign word
            // for negative EAX. Negating it produces +1 and breaks signed math.
            let mut v = reg(abi::EAX);
            v.extend(i32c(31));
            v.push(Op::I32_SHR_S);
            out.extend(set_reg(abi::EDX, v));
        }
        crate::x86::types::Mnem::Sahf => {
            let mut v = reg(abi::FLAGS);
            v.extend(i32c(0xffff_ff00u32 as i32));
            v.push(Op::I32_AND);
            v.extend(reg(abi::EAX));
            v.extend(i32c(8));
            v.push(Op::I32_SHR_U);
            v.extend(i32c(0xff));
            v.push(Op::I32_AND);
            v.push(Op::I32_OR);
            out.extend(set_reg(abi::FLAGS, v));
        }
        crate::x86::types::Mnem::Lahf => {
            let mut v = reg(abi::EAX);
            v.extend(i32c(0xffff_00ffu32 as i32));
            v.push(Op::I32_AND);
            v.extend(reg(abi::FLAGS));
            v.extend(i32c(0xff));
            v.push(Op::I32_AND);
            v.extend(i32c(2));
            v.push(Op::I32_OR);
            v.extend(i32c(8));
            v.push(Op::I32_SHL);
            v.push(Op::I32_OR);
            out.extend(set_reg(abi::EAX, v));
        }
        crate::x86::types::Mnem::Ud2 => {
            out.extend(set_reg(abi::TRAP_VA, i32c(insn.va as i32)));
            out.push(Op::UNREACHABLE);
        }
        crate::x86::types::Mnem::Int3 => out.push(Op::UNREACHABLE),
        crate::x86::types::Mnem::Rdtsc => return Ok(helper_call(abi::fpu_helper::RDTSC)),
        crate::x86::types::Mnem::Cpuid => return Ok(helper_call(abi::fpu_helper::CPUID)),
        _ => {
            return Err(EmitError::new(
                insn.va,
                format!("unsupported mnemonic {:?}", insn.mnem),
            ))
        }
    }
    Ok(out)
}
