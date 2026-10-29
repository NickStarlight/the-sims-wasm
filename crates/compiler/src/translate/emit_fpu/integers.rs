//! x87 integer loads and conversions.

use super::{
    abi, emit_address, f64c, i32c, i64c, load_f64, local_get, local_set, pop_top, push_top,
    read_f64, reg, round_integer, set_reg, slot, store_f64, store_int, MemOp, Op, Seq, FTMP,
};

/// DF memory: FILD/FIST/FISTP m16, FILD/FISTP m64 (BCD forms return an unsupported-instruction error).
pub(super) fn dfmem(reg_field: u8, m: &MemOp) -> Result<Seq, String> {
    Ok(match reg_field {
        0 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), read_f64(m, "i16")));
            v
        }
        2 => store_int(m, true),
        3 => {
            let mut v = store_int(m, true);
            v.extend(pop_top());
            v
        }
        4 => return Err("FBLD unsupported".into()),
        5 => {
            let mut v = push_top();
            let ld = {
                let mut x = emit_address(m);
                x.push(Op::I64_LOAD);
                x.push(3);
                x.push(0);
                x.push(Op::F64_CONVERT_I64_S);
                x
            };
            v.extend(store_f64(slot(0), ld));
            v
        }
        6 => return Err("FBSTP unsupported".into()),
        7 => {
            // DF /7 is FISTP m64int. FSTP m80 is DB /7; confusing
            // them also overwrites two bytes beyond the integer destination.
            let mut v = emit_address(m);
            v.extend(round_integer(load_f64(slot(0))));
            v.extend(local_set(FTMP));
            v.extend(local_get(FTMP));
            v.extend(f64c(-9223372036854775808.0));
            v.push(Op::F64_GE);
            v.extend(local_get(FTMP));
            v.extend(f64c(9223372036854775808.0));
            v.push(Op::F64_LT);
            v.push(Op::I32_AND);
            v.extend([Op::IF, 0x7e]); // i64 result; ordered comparisons reject NaN
            v.extend(local_get(FTMP));
            v.push(Op::I64_TRUNC_F64_S);
            v.push(Op::ELSE);
            // Masked invalid conversion stores the x87 integer indefinite.
            let mut status = reg(abi::FP_STATUS);
            status.extend(i32c(1));
            status.push(Op::I32_OR);
            v.extend(set_reg(abi::FP_STATUS, status));
            v.extend(i64c(i64::MIN));
            v.push(Op::END);
            v.push(Op::I64_STORE);
            v.push(3);
            v.push(0);
            v.extend(pop_top());
            v
        }
        _ => return Err(format!("FPU DF /{} unsupported", reg_field)),
    })
}
