//! Full-width arithmetic flags, carry propagation and double shifts.
use super::{
    abi, i32c, local_get, local_set, pick, reg, set_flags, sleb, zs_pf, Op, Seq, A, B, FULL, RES,
    T1, T3,
};

/// add: CF = unsigned overflow, OF = signed overflow.
pub fn flag_add() -> Seq {
    let mut v = zs_pf();
    v.extend(pick(i32c(abi::CF), i32c(0), {
        // CF = sum <u a (unsigned overflow)
        let mut x = local_get(RES);
        x.extend(local_get(A));
        x.push(Op::I32_LT_U);
        x
    }));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.extend(local_get(B));
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.push(Op::I32_AND);
        x.extend(i32c(31));
        x.push(Op::I32_SHR_U);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}

/// sub/cmp: CF = borrow, OF = signed overflow.
pub fn flag_sub() -> Seq {
    let mut v = zs_pf();
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(B));
        x.push(Op::I32_LT_U);
        x
    }));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(B));
        x.push(Op::I32_XOR);
        x.extend(local_get(A));
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.push(Op::I32_AND);
        x.extend(i32c(31));
        x.push(Op::I32_SHR_U);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}

/// sbb: borrow propagates — CF = (a <u b) || (a == b && carry_in).
pub fn flag_sub_borrow() -> Seq {
    let mut v = zs_pf();
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(B));
        x.push(Op::I32_LT_U);
        x
    }));
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(B));
        x.push(Op::I32_XOR);
        x.push(Op::I32_EQZ);
        x.extend(reg(abi::FLAGS));
        x.extend(i32c(abi::CF));
        x.push(Op::I32_AND);
        x.push(Op::I32_AND);
        x
    }));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(B));
        x.push(Op::I32_XOR);
        x.extend(local_get(A));
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.push(Op::I32_AND);
        x.extend(i32c(31));
        x.push(Op::I32_SHR_U);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}

/// adc: carry propagates — CF = (res <u a) || (res == a && carry_in).
pub fn flag_add_carry() -> Seq {
    let mut v = zs_pf();
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(RES);
        x.extend(local_get(A));
        x.push(Op::I32_LT_U);
        x
    }));
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(RES);
        x.extend(local_get(A));
        x.push(Op::I32_EQ);
        x.extend(reg(abi::FLAGS));
        x.extend(i32c(abi::CF));
        x.push(Op::I32_AND);
        x.push(Op::I32_AND);
        x
    }));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(A);
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.extend(local_get(B));
        x.extend(local_get(RES));
        x.push(Op::I32_XOR);
        x.push(Op::I32_AND);
        x.extend(i32c(31));
        x.push(Op::I32_SHR_U);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}

/// SHLD/SHRD flags (call with RES/A already set, c = count&31 in local T1; the
/// caller guards on c≠0 — count 0 changes no flags).
pub fn flag_sh_double(left: bool) -> Seq {
    let mut v = zs_pf();
    v.extend(pick(
        i32c(abi::CF),
        i32c(0),
        if left {
            let mut x = local_get(A);
            x.extend(i32c(32));
            x.extend(local_get(T1));
            x.extend(i32c(31));
            x.push(Op::I32_AND);
            x.push(Op::I32_SUB);
            x.push(Op::I32_SHR_U);
            x.extend(i32c(1));
            x.push(Op::I32_AND);
            x
        } else {
            let mut x = local_get(A);
            x.extend(local_get(T1));
            x.extend(i32c(31));
            x.push(Op::I32_AND);
            x.extend(i32c(1));
            x.push(Op::I32_SUB);
            x.push(Op::I32_SHR_U);
            x.extend(i32c(1));
            x.push(Op::I32_AND);
            x
        },
    ));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(RES);
        x.extend(local_get(A));
        x.push(Op::I32_XOR);
        x.extend(i32c(31));
        x.push(Op::I32_SHR_U);
        x.extend(local_get(T1));
        x.extend(i32c(1));
        x.push(Op::I32_EQ);
        x.push(Op::I32_AND);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}

/// inc/dec: CF is preserved.
pub fn flag_inc_dec(is_add: bool) -> Seq {
    let mut v = reg(abi::FLAGS);
    v.extend(i32c(abi::CF));
    v.push(Op::I32_AND);
    v.extend(local_set(T3));
    v.extend(if is_add { flag_add() } else { flag_sub() });
    // clear the newly computed CF (x86 inc/dec preserve CF), restore the saved one
    v.extend(reg(abi::FLAGS));
    v.extend(i32c(!abi::CF));
    v.push(Op::I32_AND);
    v.extend(local_get(T3));
    v.push(Op::I32_OR);
    set_flags(v)
}

/// imul low 32 bits: CF=OF when the full product doesn't fit.
pub fn flag_mul() -> Seq {
    let mut v = local_get(A);
    v.push(Op::I64_EXTEND_I32_S);
    v.extend(local_get(B));
    v.push(Op::I64_EXTEND_I32_S);
    v.push(Op::I64_MUL);
    v.push(Op::LOCAL_TEE);
    v.extend(sleb(FULL as i32));
    v.push(Op::I32_WRAP_I64);
    v.extend(local_set(RES));
    v.extend(pick(i32c(abi::CF | abi::OF), i32c(0), {
        let mut x = local_get(FULL);
        x.extend(local_get(RES));
        x.push(Op::I64_EXTEND_I32_S);
        x.push(Op::I64_NE);
        x
    }));
    v.extend(zs_pf());
    v.push(Op::I32_OR);
    set_flags(v)
}

/// neg: 0-a; CF = a != 0; OF = a was INT_MIN.
pub fn flag_neg() -> Seq {
    let mut v = i32c(0);
    v.extend(local_get(A));
    v.push(Op::I32_SUB);
    v.extend(local_set(RES));
    v.extend(zs_pf());
    // x86 NEG: CF = 1 iff operand != 0 (was inverted — broke neg/sbb/and thunk idiom)
    v.extend(pick(i32c(abi::CF), i32c(0), {
        let mut x = local_get(A);
        x.extend(i32c(0));
        x.push(Op::I32_NE);
        x
    }));
    v.extend(pick(i32c(abi::OF), i32c(0), {
        let mut x = local_get(A);
        x.extend(i32c(-2147483648));
        x.push(Op::I32_EQ);
        x
    }));
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    set_flags(v)
}
