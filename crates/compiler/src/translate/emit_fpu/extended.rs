//! Conversion between the x87 memory format and the binary64 arithmetic bank.

use super::{
    emit_address, i32c, i64c, load_f64, local_get, local_set, slot, MemOp, Op, Seq, FULL, M52, T1,
};
use crate::translate::layout::{T2, T3};

fn binary(mut left: Seq, right: Seq, op: u8) -> Seq {
    left.extend(right);
    left.push(op);
    left
}

fn choose(condition: Seq, yes: Seq, no: Seq) -> Seq {
    let mut v = condition;
    v.extend([Op::IF, 0x7e]);
    v.extend(yes);
    v.push(Op::ELSE);
    v.extend(no);
    v.push(Op::END);
    v
}

fn exponent() -> Seq {
    binary(local_get(T2), i32c(0x7fff), Op::I32_AND)
}

/// Round an unsigned 64-bit significand right by 1..63 bits, ties to even.
fn rounded(shift: Seq) -> Seq {
    let quotient = binary(local_get(FULL), shift.clone(), Op::I64_SHR_U);
    let mask = binary(binary(i64c(1), shift.clone(), Op::I64_SHL), i64c(1), 0x7d);
    let remainder = binary(local_get(FULL), mask, Op::I64_AND);
    let half = binary(i64c(1), binary(shift, i64c(1), 0x7d), Op::I64_SHL);
    let above = binary(remainder.clone(), half.clone(), 0x56); // i64.gt_u
    let tied_odd = binary(
        binary(remainder, half, Op::I64_EQ),
        binary(
            binary(quotient.clone(), i64c(1), Op::I64_AND),
            i64c(1),
            Op::I64_EQ,
        ),
        Op::I32_AND,
    );
    let mut increment = binary(above, tied_odd, Op::I32_OR);
    increment.push(Op::I64_EXTEND_I32_U);
    binary(quotient, increment, Op::I64_ADD)
}

/// FLD m80: round the explicit unsigned significand into binary64.
/// The bank cannot retain x87's extra precision or exponent range; conversion
/// must still preserve every representable value, including signed subnormals.
pub(super) fn load_m80(m: &MemOp) -> Seq {
    load_m80_from(emit_address(m))
}

pub(super) fn load_m80_from(addr: Seq) -> Seq {
    let mut v = addr;
    v.extend(local_set(T1));
    v.extend(local_get(T1));
    v.extend([Op::I64_LOAD, 0, 0]);
    v.extend(local_set(FULL));
    v.extend(local_get(T1));
    v.extend([Op::I32_LOAD16_U, 0, 8]);
    v.extend(local_set(T2));

    let normal_exponent = {
        let mut e = binary(exponent(), i32c(15361), Op::I32_SUB);
        e.push(Op::I64_EXTEND_I32_U);
        binary(e, i64c(52), Op::I64_SHL)
    };
    let normal = binary(normal_exponent, rounded(i64c(11)), Op::I64_ADD);
    let subnormal = {
        let mut shift = binary(i32c(15372), exponent(), Op::I32_SUB);
        shift.push(Op::I64_EXTEND_I32_U);
        rounded(shift)
    };
    let tiny = choose(
        binary(exponent(), i32c(15308), Op::I32_EQ),
        choose(
            binary(local_get(FULL), i64c(i64::MIN), 0x56),
            i64c(1),
            i64c(0),
        ),
        i64c(0),
    );
    let finite = choose(
        binary(exponent(), i32c(17407), Op::I32_GE_U),
        i64c(0x7ff0_0000_0000_0000),
        choose(
            binary(exponent(), i32c(15361), Op::I32_GE_U),
            normal,
            choose(
                binary(exponent(), i32c(15309), Op::I32_GE_U),
                subnormal,
                tiny,
            ),
        ),
    );
    let special = choose(
        binary(local_get(FULL), i64c(i64::MIN), Op::I64_EQ),
        i64c(0x7ff0_0000_0000_0000),
        i64c(0x7ff8_0000_0000_0000),
    );
    let magnitude = choose(
        {
            let mut e = exponent();
            e.push(Op::I32_EQZ);
            e
        },
        i64c(0),
        choose(
            binary(exponent(), i32c(0x7fff), Op::I32_EQ),
            special,
            choose(
                binary(
                    binary(local_get(FULL), i64c(63), Op::I64_SHR_S),
                    i64c(0),
                    Op::I64_NE,
                ),
                finite,
                i64c(0x7ff8_0000_0000_0000), // unsupported unnormal encoding
            ),
        ),
    );
    let sign = {
        let mut s = binary(local_get(T2), i32c(15), Op::I32_SHR_U);
        s.push(Op::I64_EXTEND_I32_U);
        binary(s, i64c(63), Op::I64_SHL)
    };
    v.extend(binary(magnitude, sign, Op::I64_OR));
    v.push(Op::F64_REINTERPRET_I64);
    v
}

pub(super) fn store_m80(m: &MemOp) -> Seq {
    store_m80_to(emit_address(m), 0)
}

/// FSTP/FNSAVE: all binary64 values have an exact extended representation.
pub(super) fn store_m80_to(addr: Seq, slot_idx: i32) -> Seq {
    let mut v = addr;
    v.extend(local_set(T1));
    v.extend(load_f64(slot(slot_idx)));
    v.push(Op::I64_REINTERPRET_F64);
    v.extend(local_set(FULL));
    let mut exp11 = binary(local_get(FULL), i64c(52), Op::I64_SHR_U);
    exp11.extend(i64c(0x7ff));
    exp11.extend([Op::I64_AND, Op::I32_WRAP_I64]);
    v.extend(exp11);
    v.extend(local_set(T2));
    let fraction = binary(local_get(FULL), i64c(M52), Op::I64_AND);
    let mut leading = fraction.clone();
    leading.extend([0x79, Op::I32_WRAP_I64]); // i64.clz
    v.extend(leading);
    v.extend(local_set(T3));
    let mantissa = choose(
        {
            let mut e = local_get(T2);
            e.push(Op::I32_EQZ);
            e
        },
        choose(
            {
                let mut f = fraction.clone();
                f.push(Op::I64_EQZ);
                f
            },
            i64c(0),
            {
                let mut shift = local_get(T3);
                shift.push(Op::I64_EXTEND_I32_U);
                binary(fraction.clone(), shift, Op::I64_SHL)
            },
        ),
        binary(
            binary(fraction, i64c(11), Op::I64_SHL),
            i64c(i64::MIN),
            Op::I64_OR,
        ),
    );
    v.extend(local_get(T1));
    v.extend(mantissa);
    v.extend([Op::I64_STORE, 0, 0]);
    let exp15 = choose(
        {
            let mut e = local_get(T2);
            e.push(Op::I32_EQZ);
            e
        },
        choose(
            {
                let mut f = binary(local_get(FULL), i64c(M52), Op::I64_AND);
                f.push(Op::I64_EQZ);
                f
            },
            i64c(0),
            {
                let mut e = binary(i32c(15372), local_get(T3), Op::I32_SUB);
                e.push(Op::I64_EXTEND_I32_U);
                e
            },
        ),
        choose(
            binary(local_get(T2), i32c(0x7ff), Op::I32_EQ),
            i64c(0x7fff),
            {
                let mut e = binary(local_get(T2), i32c(15360), Op::I32_ADD);
                e.push(Op::I64_EXTEND_I32_U);
                e
            },
        ),
    );
    let sign = binary(
        binary(local_get(FULL), i64c(63), Op::I64_SHR_U),
        i64c(15),
        Op::I64_SHL,
    );
    v.extend(local_get(T1));
    v.extend(binary(exp15, sign, Op::I64_OR));
    v.extend([Op::I32_WRAP_I64, Op::I32_STORE16, 0, 8]);
    v
}
