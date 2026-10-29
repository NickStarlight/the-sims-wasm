//! 80-bit extended memory representation conversion.

use super::{
    emit_address, f64_bytes, i32c, i64c, load_f64, local_get, local_set, sleb, slot, MemOp, Op,
    Seq, FTMP, FULL, M52, T1,
};

/// m80 sign bits (0/1 i32) from the packed sign+exp halfword.
pub(super) fn m80_sign(se: &Seq) -> Seq {
    let mut v = se.clone();
    v.extend(i32c(15));
    v.push(Op::I32_SHR_U);
    v
}

pub(super) fn m80_sign_bits(se: &Seq) -> Seq {
    let mut v = m80_sign(se);
    v.push(Op::I64_EXTEND_I32_U);
    v.extend(i64c(63));
    v.push(Op::I64_SHL);
    v
}

/// FLD m80: 80-bit extended → f64.
pub(super) fn load_m80(m: &MemOp) -> Seq {
    load_m80_from(emit_address(m))
}

/// m80 load from an explicit address seq (FRSTOR's per-register loads).
pub(super) fn load_m80_from(addr: Seq) -> Seq {
    let addr0 = addr;
    let mant = {
        let mut v = addr0.clone();
        v.push(Op::I64_LOAD);
        v.push(3);
        v.push(0);
        v
    };
    let se = {
        let mut v = addr0.clone();
        v.extend(i32c(8));
        v.push(Op::I32_ADD);
        v.push(Op::I32_LOAD16_U);
        v.push(1);
        v.push(0);
        v
    };
    let exp = {
        let mut v = se.clone();
        v.extend(i32c(0x7fff));
        v.push(Op::I32_AND);
        v
    };
    // mantF64 = f64(mant>>1) * 2 + f64(mant&1) ∈ [2^63, 2^64)
    let mant_f64 = {
        let mut v = mant.clone();
        v.extend(local_set(FULL));
        v.extend(local_get(FULL));
        v.push(Op::I64_CONST);
        v.extend(sleb(1));
        v.push(Op::I64_SHR_S);
        v.push(Op::F64_CONVERT_I64_S);
        v.push(Op::F64_CONST);
        v.extend(f64_bytes(2.0));
        v.push(Op::F64_MUL);
        v.extend(local_get(FULL));
        v.push(Op::I64_CONST);
        v.extend(sleb(1));
        v.push(Op::I64_AND);
        v.push(Op::F64_CONVERT_I64_S);
        v.push(Op::F64_ADD);
        v
    };
    let e = {
        let mut v = exp.clone();
        v.extend(i32c(16446));
        v.push(Op::I32_SUB);
        v
    };
    let sign_bits = m80_sign_bits(&se);
    let normal_bits = {
        let mut v = mant_f64;
        v.push(Op::I64_REINTERPRET_F64);
        v.extend(e.clone());
        v.push(Op::I64_EXTEND_I32_S);
        v.extend(i64c(52));
        v.push(Op::I64_SHL);
        v.push(Op::I64_ADD);
        v.extend(sign_bits.clone());
        v.push(Op::I64_OR);
        v
    };
    let inf_bits = {
        let mut v = i64c(0x7ff0_0000_0000_0000u64 as i64);
        v.extend(sign_bits.clone());
        v.push(Op::I64_OR);
        v
    };
    let nan_bits = {
        let mut v = i64c(0x7ff8_0000_0000_0000u64 as i64);
        v.extend(sign_bits.clone());
        v.push(Op::I64_OR);
        v
    };
    let inf_or_nan = {
        let mut v = inf_bits;
        v.extend(nan_bits);
        v.extend(mant.clone());
        v.push(Op::I64_EQZ);
        v.push(Op::SELECT);
        v
    };
    let is_special = {
        let mut v = exp.clone();
        v.push(Op::I32_EQZ);
        v.extend(e.clone());
        v.extend(i32c(960));
        v.push(Op::I32_GT_U);
        v.push(Op::I32_OR);
        v.extend(e);
        v.extend(i32c(-1085));
        v.push(Op::I32_LT_S);
        v.push(Op::I32_OR);
        v
    };
    let mut v = sign_bits; // ±0
    v.extend(inf_or_nan);
    v.extend(exp.clone());
    v.push(Op::I32_EQZ);
    v.push(Op::SELECT); // exp==0 → zero, else inf/nan
    v.extend(normal_bits);
    v.extend(is_special);
    v.push(Op::SELECT);
    v.push(Op::F64_REINTERPRET_I64);
    v
}

/// FSTP m80: f64 → 80-bit extended.
pub(super) fn store_m80(m: &MemOp) -> Seq {
    store_m80_to(emit_address(m), 0)
}

/// m80 store of st(slotIdx) to the explicit address `addr` (FNSAVE shares it).
pub(super) fn store_m80_to(addr: Seq, slot_idx: i32) -> Seq {
    let exp11 = {
        let mut v = local_get(FULL);
        v.extend(i64c(52));
        v.push(Op::I64_SHR_U);
        v.extend(i64c(0x7ff));
        v.push(Op::I64_AND);
        v
    };
    let implicit_bit = {
        let mut v = i64c(1i64 << 63);
        v.extend(i64c(0));
        let cond = {
            let mut c = exp11.clone();
            c.push(Op::I64_EQZ);
            c.extend(exp11.clone());
            c.extend(i64c(0x7ff));
            c.push(Op::I64_EQ);
            c.push(Op::I32_OR);
            c.push(Op::I32_EQZ);
            c
        };
        v.extend(cond);
        v.push(Op::SELECT);
        v
    };
    let exp15 = {
        let mut v = i64c(0);
        v.extend({
            let mut x = exp11.clone();
            x.extend(i64c(15360));
            x.push(Op::I64_ADD);
            x
        });
        let cond = {
            let mut c = exp11.clone();
            c.push(Op::I64_EQZ);
            c.push(Op::I32_EQZ);
            c
        };
        v.extend(cond);
        v.push(Op::SELECT);
        v
    };
    let mut v = addr;
    v.extend(local_set(T1));
    v.extend(load_f64(slot(slot_idx)));
    v.extend(local_set(FTMP));
    v.extend(local_get(FTMP));
    v.push(Op::I64_REINTERPRET_F64);
    v.extend(local_set(FULL));
    // [T1]: mant80
    v.extend(local_get(T1));
    v.extend(local_get(FULL));
    v.extend(i64c(M52));
    v.push(Op::I64_AND);
    v.extend(i64c(11));
    v.push(Op::I64_SHL);
    v.extend(implicit_bit);
    v.push(Op::I64_OR);
    v.push(Op::I64_STORE);
    v.push(3);
    v.push(0);
    // [T1+8]: (sign << 15) | exp15
    v.extend(local_get(T1));
    v.extend(i32c(8));
    v.push(Op::I32_ADD);
    v.extend(local_get(FULL));
    v.extend(i64c(63));
    v.push(Op::I64_SHR_U);
    v.extend(i64c(15));
    v.push(Op::I64_SHL);
    v.extend(exp15);
    v.push(Op::I64_OR);
    v.push(Op::I32_WRAP_I64);
    v.push(Op::I32_STORE16);
    v.push(1);
    v.push(0);
    v
}
