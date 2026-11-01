//! x87 memory operand opcode families.

use super::{
    abi, emit_address, fnsave, frstor, i32c, load_m80, pop_top, push_top, read_f64, reg, set_reg,
    slot, status_word, store16, store_f64, store_f64m, store_int, store_m80, MemOp, Op, Seq,
};

/// D9 memory forms.
pub(super) fn d9mem(reg_field: u8, m: &MemOp) -> Result<Seq, String> {
    Ok(match reg_field {
        0 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), read_f64(m, "f32")));
            v
        }
        2 => store_f64m(m, false),
        3 => {
            let mut v = store_f64m(m, false);
            v.extend(pop_top());
            v
        }
        4 => fldenv(m),
        5 => load_control(m),
        6 => fnstenv(m),
        7 => store16(m, reg(abi::FP_CONTROL)),
        _ => return Err(format!("FPU D9 /{} unsupported", reg_field)),
    })
}

pub(super) fn load_control(m: &MemOp) -> Seq {
    let mut value = emit_address(m);
    value.extend([Op::I32_LOAD16_U, 1, 0]);
    set_reg(abi::FP_CONTROL, value)
}

/// FLDENV: restore control and status from the 32-bit environment layout.
pub(super) fn fldenv(m: &MemOp) -> Seq {
    let sw = {
        let mut v = emit_address(m);
        v.extend(i32c(4));
        v.push(Op::I32_ADD);
        v.push(Op::I32_LOAD16_U);
        v.push(1);
        v.push(0);
        v
    };
    let st = |v: Seq, f: &dyn Fn(&mut Seq)| {
        let mut x: Seq = v;
        f(&mut x);
        x
    };
    let sw1 = sw.clone();
    let mut out = load_control(m);
    out.extend(set_reg(
        abi::FP_STATUS,
        st(sw1, &|x| {
            x.extend(i32c(0xc7ff));
            x.push(Op::I32_AND);
        }),
    ));
    let sw2 = sw;
    out.extend(set_reg(
        abi::FP_TOP,
        st(sw2, &|x| {
            x.extend(i32c(11));
            x.push(Op::I32_SHR_U);
            x.extend(i32c(7));
            x.push(Op::I32_AND);
        }),
    ));
    out
}

/// FNSTENV: store modeled control and status in the 32-bit environment layout.
pub(super) fn fnstenv(m: &MemOp) -> Seq {
    let addr = emit_address(m);
    let mut out = store16(m, reg(abi::FP_CONTROL));
    out.extend(addr);
    out.extend(i32c(4));
    out.push(Op::I32_ADD);
    out.extend(status_word());
    out.push(Op::I32_STORE16);
    out.push(1);
    out.push(0);
    out
}

/// DB memory: FILD/FIST/FISTP m32, FLD/FSTP m80.
pub(super) fn dbmem(reg_field: u8, m: &MemOp) -> Result<Seq, String> {
    Ok(match reg_field {
        0 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), read_f64(m, "i32")));
            v
        }
        2 => store_int(m, false),
        3 => {
            let mut v = store_int(m, false);
            v.extend(pop_top());
            v
        }
        5 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), load_m80(m)));
            v
        }
        7 => {
            let mut v = store_m80(m);
            v.extend(pop_top());
            v
        }
        _ => return Err(format!("FPU DB /{} unsupported", reg_field)),
    })
}

/// DD memory: FLD/FST/FSTP m64, FNSAVE m108, FRSTOR m108, FNSTSW m16.
pub(super) fn ddmem(reg_field: u8, m: &MemOp) -> Result<Seq, String> {
    Ok(match reg_field {
        0 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), read_f64(m, "f64")));
            v
        }
        2 => store_f64m(m, true),
        3 => {
            let mut v = store_f64m(m, true);
            v.extend(pop_top());
            v
        }
        4 => frstor(m),
        6 => fnsave(m),
        7 => store16(m, status_word()),
        _ => return Err(format!("FPU DD /{} unsupported", reg_field)),
    })
}
