//! x87 binary arithmetic and register-pop operations.

use super::{fcom, load_f64, pop_top, set_st0, slot, store_f64, Op, Seq};

pub(super) fn bin_st0(op: u8, value: Seq) -> Seq {
    let mut v = load_f64(slot(0));
    v.extend(value);
    v.push(op);
    set_st0(v)
}

/// Register-to-register op on st(i) with pop semantics (DE family).
pub(super) fn bin_st_i_pop(reg_field: u8, i: i32) -> Seq {
    let normal = |op: u8| {
        let mut v = load_f64(slot(i));
        v.extend(load_f64(slot(0)));
        v.push(op);
        let mut out = store_f64(slot(i), v);
        out.extend(pop_top());
        out
    };
    let reverse = |op: u8| {
        let mut v = load_f64(slot(0));
        v.extend(load_f64(slot(i)));
        v.push(op);
        let mut out = store_f64(slot(i), v);
        out.extend(pop_top());
        out
    };
    match reg_field {
        0 => normal(Op::F64_ADD),
        1 => normal(Op::F64_MUL),
        2 => fcom(load_f64(slot(i))), // reserved: behaves as FCOMP w/o pop
        3 => {
            let mut v = fcom(load_f64(slot(i))); // reserved: FCOMP
            v.extend(pop_top());
            v
        }
        4 => reverse(Op::F64_SUB), // FSUBRP
        5 => normal(Op::F64_SUB),  // FSUBP
        6 => reverse(Op::F64_DIV), // FDIVRP
        7 => normal(Op::F64_DIV),  // FDIVP
        _ => panic!("binStIPop: bad op {}", reg_field),
    }
}
