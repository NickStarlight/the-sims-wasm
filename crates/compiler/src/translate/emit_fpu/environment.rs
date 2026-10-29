//! x87 environment save and restore.

use super::{
    abi, emit_address, i32c, load_m80_from, reg, set_reg, slot, status_word, store_f64,
    store_m80_to, MemOp, Op, Seq, CONTROL_WORD,
};

pub(super) fn add_off(addr: Seq, off: i32) -> Seq {
    let mut v = addr;
    v.extend(i32c(off));
    v.push(Op::I32_ADD);
    v
}

pub(super) fn store16_at(addr: Seq, value: Seq) -> Seq {
    let mut v = addr;
    v.extend(value);
    v.push(Op::I32_STORE16);
    v.push(1);
    v.push(0);
    v
}

pub(super) fn store32_at(addr: Seq, value: Seq) -> Seq {
    let mut v = addr;
    v.extend(value);
    v.push(Op::I32_STORE);
    v.push(2);
    v.push(0);
    v
}

/// FNSAVE m108 (DD /6).
pub(super) fn fnsave(m: &MemOp) -> Seq {
    let addr = emit_address(m);
    let mut out = store16_at(addr.clone(), reg(abi::FP_CONTROL));
    out.extend(store16_at(add_off(addr.clone(), 4), status_word()));
    out.extend(store16_at(add_off(addr.clone(), 8), i32c(0xffff)));
    for off in (12..28).step_by(4) {
        out.extend(store32_at(add_off(addr.clone(), off), i32c(0)));
    }
    for j in 0..8i32 {
        out.extend(store_m80_to(
            add_off(addr.clone(), 28 + 10 * j),
            (8 - j) & 7,
        ));
    }
    out.extend(set_reg(abi::FP_TOP, i32c(0)));
    out.extend(set_reg(abi::FP_STATUS, i32c(0)));
    out.extend(set_reg(abi::FP_CONTROL, i32c(CONTROL_WORD)));
    out
}

/// FRSTOR m108 (DD /4).
pub(super) fn frstor(m: &MemOp) -> Seq {
    let addr = emit_address(m);
    let fsw = {
        let mut v = addr.clone();
        v.extend(i32c(4));
        v.push(Op::I32_ADD);
        v.push(Op::I32_LOAD16_U);
        v.push(1);
        v.push(0);
        v
    };
    let st = |mut x: Seq, f: &dyn Fn(&mut Seq)| {
        f(&mut x);
        x
    };
    let mut out = super::memory::load_control(m);
    out.extend(set_reg(
        abi::FP_STATUS,
        st(fsw.clone(), &|x| {
            x.extend(i32c(0xc7ff));
            x.push(Op::I32_AND);
        }),
    ));
    out.extend(set_reg(
        abi::FP_TOP,
        st(fsw, &|x| {
            x.extend(i32c(11));
            x.push(Op::I32_SHR_U);
            x.extend(i32c(7));
            x.push(Op::I32_AND);
        }),
    ));
    for j in 0..8i32 {
        out.extend(store_f64(
            slot((8 - j) & 7),
            load_m80_from(add_off(addr.clone(), 28 + 10 * j)),
        ));
    }
    out
}
