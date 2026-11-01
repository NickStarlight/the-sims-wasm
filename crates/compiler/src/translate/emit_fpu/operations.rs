//! x87 scale, remainder, stack exchange and helper operations.

use super::{
    abi, f64c, i32c, i64c, load_f64, local_get, local_set, push_top, sel, set_reg, set_st0, slot,
    store_f64, Op, Seq, C2, FTMP,
};

/// (slot(i) op slot(0)) or (slot(0) op slot(i)) when reversed.
pub(super) fn bin_parts(i: i32, reversed: u8, op: u8) -> Seq {
    let mut v = if reversed == 1 {
        load_f64(slot(0))
    } else {
        load_f64(slot(i))
    };
    v.extend(if reversed == 1 {
        load_f64(slot(i))
    } else {
        load_f64(slot(0))
    });
    v.push(op);
    v
}

pub(super) fn unop_st0(op: u8) -> Seq {
    let mut v = load_f64(slot(0));
    v.push(op);
    v
}

pub(super) fn helper(k: i32) -> Seq {
    let mut v = i32c(k);
    v.push(Op::CALL);
    v.push(0);
    v
}

/// FSCALE: st(0) *= 2^trunc(st(1)).
pub(super) fn fscale() -> Seq {
    let st0_bits = {
        let mut v = load_f64(slot(0));
        v.push(Op::I64_REINTERPRET_F64);
        v
    };
    let st1 = load_f64(slot(1));
    let is_nan1 = {
        let mut v = st1.clone();
        v.extend(st1.clone());
        v.push(Op::F64_NE);
        v
    };
    let st0_is_nan = {
        let mut v = load_f64(slot(0));
        v.extend(load_f64(slot(0)));
        v.push(Op::F64_NE);
        v
    };
    let clamped1 = sel(f64c(2100.0), st1.clone(), {
        let mut x = st1.clone();
        x.extend(f64c(2100.0));
        x.push(Op::F64_GT);
        x
    });
    let clamped2 = sel(f64c(-2100.0), clamped1.clone(), {
        let mut x = clamped1;
        x.extend(f64c(-2100.0));
        x.push(Op::F64_LT);
        x
    });
    let k = {
        let mut v = sel(f64c(0.0), clamped2, is_nan1.clone());
        v.push(Op::I64_TRUNC_F64_S); // st(1) is clamped to ±2100 with NaN → 0 before trunc
        v
    };
    let sign = {
        let mut v = st0_bits.clone();
        v.extend(i64c(0x8000_0000_0000_0000u64 as i64));
        v.push(Op::I64_AND);
        v
    };
    let new_exp = {
        let mut v = st0_bits.clone();
        v.extend(i64c(52));
        v.push(Op::I64_SHR_U);
        v.extend(i64c(0x7ff));
        v.push(Op::I64_AND);
        v.extend(k);
        v.push(Op::I64_ADD);
        v
    };
    let normal = {
        let mut v = st0_bits.clone();
        v.extend(i64c(0x800f_ffff_ffff_ffffu64 as i64));
        v.push(Op::I64_AND);
        v.extend(new_exp.clone());
        v.extend(i64c(52));
        v.push(Op::I64_SHL);
        v.push(Op::I64_OR);
        v
    };
    let inf = {
        let mut v = i64c(0x7ff0_0000_0000_0000u64 as i64);
        v.extend(sign.clone());
        v.push(Op::I64_OR);
        v
    };
    let inner = sel(sign.clone(), normal, {
        let mut c = new_exp.clone();
        c.extend(i64c(0));
        c.push(Op::I64_LE_S);
        c
    });
    let special = {
        // sel(inf, inner, newExp >= 0x7ff) = [inf, inner, cond, select]
        let mut v = inf;
        v.extend(inner);
        v.extend(new_exp.clone());
        v.extend(i64c(0x7ff));
        v.push(Op::I64_GE_S);
        v.push(Op::SELECT);
        v
    };
    let result = {
        // sel(0x7ff8…, special, st0IsNan || isNaN1)
        let mut v = i64c(0x7ff8_0000_0000_0000u64 as i64);
        v.extend(special);
        let mut c = st0_is_nan;
        c.extend(is_nan1);
        c.push(Op::I32_OR);
        v.extend(c);
        v.push(Op::SELECT);
        v
    };
    let mut v = result;
    v.push(Op::F64_REINTERPRET_I64);
    store_f64(slot(0), v)
}

/// FPREM (nearest=false) / FPREM1 (nearest=true).
pub(super) fn fprem(nearest: bool) -> Seq {
    let st0 = load_f64(slot(0));
    let st1 = load_f64(slot(1));
    let safe_div = sel(st1.clone(), f64c(1.0), {
        let mut c = st1.clone();
        c.extend(f64c(0.0));
        c.push(Op::F64_EQ);
        c
    });
    let ratio = {
        let mut v = st0.clone();
        v.extend(safe_div);
        v.push(Op::F64_DIV);
        v
    };
    let q = {
        let mut v = ratio.clone();
        v.push(if nearest {
            Op::F64_NEAREST
        } else {
            Op::F64_TRUNC
        });
        v
    };
    let r = {
        let mut v = st0.clone();
        v.extend(st1);
        v.extend(q);
        v.push(Op::F64_MUL);
        v.push(Op::F64_SUB);
        v
    };
    let big = {
        let mut v = ratio;
        v.extend(f64c(9.223372036854776e18));
        v.push(Op::F64_GE);
        v
    };
    let mut out = set_reg(abi::FP_STATUS, sel(i32c(C2), i32c(0), big.clone()));
    out.extend(set_st0(sel(st0, r, big)));
    out
}

pub(super) fn load_st_and_push(i: i32) -> Seq {
    let mut v = load_f64(slot(i));
    v.extend(local_set(FTMP));
    v.extend(push_top());
    v.extend(slot(0));
    v.extend(local_get(FTMP));
    v.push(Op::F64_STORE);
    v.push(3);
    v.push(0);
    v
}

pub(super) fn fxch(i: i32) -> Seq {
    let mut v = load_f64(slot(i));
    v.extend(local_set(FTMP));
    v.extend(slot(i));
    v.extend(load_f64(slot(0)));
    v.push(Op::F64_STORE);
    v.push(3);
    v.push(0);
    v.extend(slot(0));
    v.extend(local_get(FTMP));
    v.push(Op::F64_STORE);
    v.push(3);
    v.push(0);
    v
}
