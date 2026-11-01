//! Integer arithmetic, multiplication, division and shifts.

use super::{
    abi, flag_mul, flag_narrow_alu, flag_narrow_shift, flag_shift, i32c, i64c, local_get,
    local_set, pick, read_op, reg, set_reg, stash, write_op, Insn, Op, Operand, Seq, A, B, FULL,
    RES, T1, T3,
};

pub(super) fn carry_in() -> Seq {
    let mut v = reg(abi::FLAGS);
    v.extend(i32c(abi::CF));
    v.push(Op::I32_AND);
    v
}

/// Binary ALU: stash a→A, b→B, compute → RES, optional writeback a, flags.
pub(super) fn binop(
    insn: &Insn,
    direct: Option<u8>,
    flags: Seq,
    writeback: bool,
    custom: Option<fn(Seq, Seq) -> Seq>,
) -> Seq {
    let a = insn.ops.first().unwrap();
    let b = insn.ops.get(1).unwrap();
    let x = local_get(A);
    let y = local_get(B);
    let res = match custom {
        Some(f) => f(x, y),
        None => {
            let mut v = x;
            v.extend(y);
            v.push(direct.unwrap());
            v
        }
    };
    let mut out = stash(a, A);
    out.extend(stash(b, B));
    out.extend(res);
    out.extend(local_set(RES));
    let size = match a {
        Operand::Reg { size, .. } => *size,
        Operand::Mem(m) => m.size,
        _ => 4,
    };
    let narrow = (size == 1 || size == 2)
        && matches!(
            insn.mnem,
            crate::x86::types::Mnem::Add
                | crate::x86::types::Mnem::Sub
                | crate::x86::types::Mnem::And
                | crate::x86::types::Mnem::Or
                | crate::x86::types::Mnem::Xor
                | crate::x86::types::Mnem::Cmp
                | crate::x86::types::Mnem::Test
                | crate::x86::types::Mnem::Adc
                | crate::x86::types::Mnem::Sbb
        );
    if narrow {
        out.extend(flag_narrow_alu(insn.mnem, size));
    }
    if writeback {
        out.extend(write_op(a, local_get(RES)));
    }
    if !narrow {
        out.extend(flags);
    }
    out
}

/// Unary ALU (inc/dec).
pub(super) fn unop(insn: &Insn, is_add: bool, flags: Seq) -> Seq {
    let a = insn.ops.first().unwrap();
    let op = if is_add { Op::I32_ADD } else { Op::I32_SUB };
    let size = match a {
        Operand::Reg { size, .. } => *size as u32,
        Operand::Mem(m) => m.size as u32,
        _ => 4,
    };
    let mut out = stash(a, A);
    out.extend(i32c(1)); // OF formula expects B = 1 for inc/dec
    out.extend(local_set(B));
    out.extend(local_get(A));
    out.extend(i32c(1));
    out.push(op);
    out.extend(local_set(RES));
    if size == 1 || size == 2 {
        out.extend(flag_narrow_alu(insn.mnem, size as u8));
    }
    out.extend(write_op(a, local_get(RES)));
    if size != 1 && size != 2 {
        out.extend(flags);
    }
    out
}

pub(super) fn sign_extend(value: Seq, size: u8) -> Seq {
    let sh = if size == 1 { 24 } else { 16 };
    let mut v = value;
    v.extend(i32c(sh));
    v.push(Op::I32_SHL);
    v.extend(i32c(sh));
    v.push(Op::I32_SHR_S);
    v
}

/// imul by operand count: 1-op edx:eax form; 2/3-op normal form.
pub(super) fn emit_imul(insn: &Insn) -> Seq {
    if insn.ops.len() == 1 {
        return emit_mul_long(true, &insn.ops[0]);
    }
    let dest = insn.ops.first().unwrap();
    let src = insn.ops.get(1).unwrap();
    if matches!(dest, Operand::Reg { size: 2, .. }) {
        let (left, right) = match insn.ops.get(2) {
            Some(immediate) => (src, immediate),
            None => (dest, src),
        };
        let mut out = sign_extend(read_op(left), 2);
        out.extend(sign_extend(read_op(right), 2));
        out.push(Op::I32_MUL); // signed 16x16 product fits a signed i32
        out.extend(local_set(RES));
        out.extend(write_op(dest, local_get(RES)));
        out.extend(reg(abi::FLAGS));
        out.extend(i32c(!(abi::CF | abi::OF)));
        out.push(Op::I32_AND);
        out.extend(pick(i32c(abi::CF | abi::OF), i32c(0), {
            let mut overflow = local_get(RES);
            overflow.extend(sign_extend(local_get(RES), 2));
            overflow.push(Op::I32_NE);
            overflow
        }));
        out.push(Op::I32_OR);
        out.extend([Op::GLOBAL_SET, abi::FLAGS]);
        return out;
    }
    match insn.ops.get(2) {
        Some(third) => {
            let mut out = stash(src, A);
            out.extend(stash(third, B));
            out.extend(local_get(A));
            out.extend(local_get(B));
            out.push(Op::I32_MUL);
            out.extend(local_set(RES));
            out.extend(write_op(dest, local_get(RES)));
            out.extend(flag_mul());
            out
        }
        None => binop(insn, Some(Op::I32_MUL), flag_mul(), true, None),
    }
}

/// MUL/IMUL one-operand: edx:eax = eax ∘ rm.
pub(super) fn emit_mul_long(signed: bool, rm: &Operand) -> Seq {
    let ext: u8 = if signed {
        Op::I64_EXTEND_I32_S
    } else {
        Op::I64_EXTEND_I32_U
    };
    let overflow: Seq = if signed {
        let mut v = local_get(FULL);
        v.extend(local_get(FULL));
        v.push(Op::I32_WRAP_I64);
        v.push(Op::I64_EXTEND_I32_S);
        v.push(Op::I64_NE);
        v
    } else {
        let mut v = local_get(FULL);
        v.extend(i64c(32));
        v.push(Op::I64_SHR_U);
        v.push(Op::I32_WRAP_I64);
        v.push(Op::I32_EQZ);
        v.extend(i32c(1));
        v.push(Op::I32_XOR);
        v
    };
    let mut out = reg(abi::EAX);
    out.push(ext);
    out.extend(read_op(rm));
    out.push(ext);
    out.push(Op::I64_MUL);
    out.extend(local_set(FULL));
    let mut eax = local_get(FULL);
    eax.push(Op::I32_WRAP_I64);
    out.extend(set_reg(abi::EAX, eax));
    let mut edx = local_get(FULL);
    edx.extend(i64c(32));
    edx.push(Op::I64_SHR_U);
    edx.push(Op::I32_WRAP_I64);
    out.extend(set_reg(abi::EDX, edx));
    let mut fl = i32c(abi::CF | 0x800);
    fl.extend(i32c(0));
    fl.extend(overflow);
    fl.push(Op::SELECT);
    out.extend(set_reg(abi::FLAGS, fl));
    out
}

/// 32-bit DIV/IDIV: edx:eax ÷ rm, with atomic divide-error handling.
pub(super) fn emit_div(signed: bool, rm: &Operand) -> Seq {
    let ext = if signed {
        Op::I64_EXTEND_I32_S
    } else {
        Op::I64_EXTEND_I32_U
    };
    let div = if signed { Op::I64_DIV_S } else { Op::I64_DIV_U };
    let rem = if signed { Op::I64_REM_S } else { Op::I64_REM_U };
    // Capture before writing EAX: rm may be EAX itself or memory addressed by it.
    let mut out = stash(rm, T1);
    out.extend(reg(abi::EDX));
    out.push(ext);
    out.extend(i64c(32));
    out.push(Op::I64_SHL);
    out.extend(reg(abi::EAX));
    // The low half is a bit pattern, even for signed division. Sign-extending
    // it would overwrite every high-half bit whenever EAX's bit31 is set.
    out.push(Op::I64_EXTEND_I32_U);
    out.push(Op::I64_OR);
    out.extend(local_set(FULL));
    let mut divisor = local_get(T1);
    divisor.push(ext);
    out.extend(local_get(FULL));
    out.extend(divisor.clone());
    out.push(rem);
    out.push(Op::I32_WRAP_I64);
    out.extend(local_set(T3));
    out.extend(local_get(FULL));
    out.extend(divisor);
    out.push(div);
    out.extend(local_set(FULL)); // dividend no longer needed; retain full quotient
                                 // x86 faults if the quotient does not fit EAX. Validate before modifying
                                 // either architectural output, including cases where i64 division succeeds.
    out.extend(local_get(FULL));
    out.extend(local_get(FULL));
    out.push(Op::I32_WRAP_I64);
    out.push(ext);
    out.push(Op::I64_NE);
    out.push(Op::IF);
    out.push(0x40);
    out.push(Op::UNREACHABLE);
    out.push(Op::END);
    let mut eax = local_get(FULL);
    eax.push(Op::I32_WRAP_I64);
    out.extend(set_reg(abi::EAX, eax));
    out.extend(set_reg(abi::EDX, local_get(T3)));
    out
}

pub(super) fn emit_shift(insn: &Insn) -> Seq {
    let a = insn.ops.first().unwrap();
    let b = insn.ops.get(1).unwrap();
    let count = match b {
        Operand::Imm(v) => i32c(*v),
        other => read_op(other),
    };
    let mut out = stash(a, A);
    out.extend(count);
    out.extend(local_set(B));
    let size = match a {
        Operand::Reg { size, .. } => *size,
        Operand::Mem(m) => m.size,
        _ => 4,
    };
    out.extend(if size < 4 {
        flag_narrow_shift(shift_kind(insn), size as u32 * 8)
    } else {
        flag_shift(shift_kind(insn))
    });
    let cond = {
        let mut v = local_get(B);
        v.extend(i32c(31));
        v.push(Op::I32_AND);
        v.push(Op::I32_EQZ);
        v
    };
    out.extend(write_op(a, pick(local_get(A), local_get(RES), cond)));
    out
}

pub(super) fn shift_kind(insn: &Insn) -> crate::translate::emit_flags::Kind {
    match insn.mnem {
        crate::x86::types::Mnem::Shl => "shl",
        crate::x86::types::Mnem::Shr => "shr",
        crate::x86::types::Mnem::Sar => "sar",
        crate::x86::types::Mnem::Rol => "rol",
        crate::x86::types::Mnem::Ror => "ror",
        crate::x86::types::Mnem::Rcl => "rcl",
        crate::x86::types::Mnem::Rcr => "rcr",
        _ => unreachable!(),
    }
}
