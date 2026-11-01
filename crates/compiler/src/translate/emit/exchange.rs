//! Register and memory exchanges.

use super::{
    abi, flag_add, i32c, local_get, local_set, pick, reg, set_reg, stash, write_op, Op, Operand,
    Seq, A, B, RES,
};

pub(super) fn emit_xadd(dest: &Operand, src: &Operand) -> Seq {
    let mut out = stash(dest, A);
    out.extend(stash(src, B));
    out.extend(local_get(A));
    out.extend(local_get(B));
    out.push(Op::I32_ADD);
    out.extend(local_set(RES));
    out.extend(write_op(src, local_get(A)));
    out.extend(write_op(dest, local_get(RES)));
    out.extend(flag_add());
    out
}

pub(super) fn emit_cmpxchg(dest: &Operand, src: &Operand) -> Seq {
    let mut out = stash(dest, A);
    out.extend(stash(src, B));
    out.extend(local_get(A));
    out.extend(reg(abi::EAX));
    out.push(Op::I32_SUB);
    out.extend(local_set(RES));
    let mut fl = i32c(0x40);
    fl.extend(i32c(0));
    fl.extend(local_get(RES));
    fl.push(Op::I32_EQZ);
    fl.push(Op::SELECT);
    out.extend(set_reg(abi::FLAGS, fl));
    out.extend(write_op(
        dest,
        pick(local_get(B), local_get(A), {
            let mut c = local_get(RES);
            c.push(Op::I32_EQZ);
            c
        }),
    ));
    out.extend(set_reg(
        abi::EAX,
        pick(reg(abi::EAX), local_get(A), {
            let mut c = local_get(RES);
            c.push(Op::I32_EQZ);
            c
        }),
    ));
    out
}

pub(super) fn emit_xchg(a: &Operand, b: &Operand) -> Seq {
    let mut out = stash(a, A);
    out.extend(stash(b, B));
    out.extend(write_op(a, local_get(B)));
    out.extend(write_op(b, local_get(A)));
    out
}
