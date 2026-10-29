//! x87 register-form opcode dispatch.

use super::{
    abi, bin_parts, bin_st_i_pop, f64c, fcom, fprem, fscale, fxam, fxch, helper, i32c, load_f64,
    load_st_and_push, pop_top, push_top, reg, round_integer, set_reg, set_st0, slot, status_word,
    store_f64, unop_st0, Op, Seq, CONTROL_WORD,
};

/// mod==3 forms.
pub(super) fn emit_fpu_reg(raw: u32, reg_field: u8, i: i32) -> Result<Seq, String> {
    // Exact encodings, before the DE arithmetic group: compare ST0/ST1,
    // then discard BOTH operands (FCOMPP / FUCOMPP).
    if raw == 0xded9 || raw == 0xdae9 {
        let mut v = fcom(load_f64(slot(1)));
        v.extend(pop_top());
        v.extend(pop_top());
        return Ok(v);
    }
    match raw & 0xfff8 {
        0xd9c0 => return Ok(load_st_and_push(i)),
        0xd9c8 => return Ok(fxch(i)),
        0xd9d0 => return Ok(Seq::new()),
        0xdec0 | 0xdec8 | 0xded0 | 0xded8 | 0xdee0 | 0xdee8 | 0xdef0 | 0xdef8 => {
            return Ok(bin_st_i_pop(reg_field & 7, i))
        }
        0xddd0 => return Ok(store_f64(slot(i), load_f64(slot(0)))),
        0xddd8 => {
            let mut v = store_f64(slot(i), load_f64(slot(0)));
            v.extend(pop_top());
            return Ok(v);
        }
        0xdde0 => return Ok(fcom(load_f64(slot(i)))),
        0xdde8 => {
            let mut v = fcom(load_f64(slot(i)));
            v.extend(pop_top());
            return Ok(v);
        }
        0xddc0 => return Ok(Seq::new()),
        0xdcc0 => return Ok(store_f64(slot(i), bin_parts(i, 0, Op::F64_ADD))),
        0xdcc8 => return Ok(store_f64(slot(i), bin_parts(i, 0, Op::F64_MUL))),
        0xdcd0 => return Ok(fcom(load_f64(slot(i)))),
        0xdcd8 => {
            let mut v = fcom(load_f64(slot(i)));
            v.extend(pop_top());
            return Ok(v);
        }
        0xdce0 => return Ok(store_f64(slot(i), bin_parts(i, 1, Op::F64_SUB))), // FSUBR = st(0)-st(i)
        0xdce8 => return Ok(store_f64(slot(i), bin_parts(i, 0, Op::F64_SUB))),
        0xdcf0 => return Ok(store_f64(slot(i), bin_parts(i, 1, Op::F64_DIV))), // FDIVR = st(0)/st(i)
        0xdcf8 => return Ok(store_f64(slot(i), bin_parts(i, 0, Op::F64_DIV))),
        0xd8c0 => return Ok(set_st0(bin_parts(i, 1, Op::F64_ADD))),
        0xd8c8 => return Ok(set_st0(bin_parts(i, 1, Op::F64_MUL))),
        0xd8d0 => return Ok(fcom(load_f64(slot(i)))),
        0xd8d8 => {
            let mut v = fcom(load_f64(slot(i)));
            v.extend(pop_top());
            return Ok(v);
        }
        0xd8e0 => return Ok(set_st0(bin_parts(i, 1, Op::F64_SUB))), // FSUB st(0),st(i)
        0xd8e8 => return Ok(set_st0(bin_parts(i, 0, Op::F64_SUB))), // FSUBR st(0),st(i)
        0xd8f0 => return Ok(set_st0(bin_parts(i, 1, Op::F64_DIV))), // FDIV st(0),st(i)
        0xd8f8 => return Ok(set_st0(bin_parts(i, 0, Op::F64_DIV))), // FDIVR st(0),st(i)
        0xd9d8 => {
            let mut v = store_f64(slot(i), load_f64(slot(0)));
            v.extend(pop_top());
            return Ok(v);
        }
        _ => {}
    }
    match raw {
        0xd9e0 => return Ok(set_st0(unop_st0(Op::F64_NEG))),
        0xd9e1 => return Ok(set_st0(unop_st0(Op::F64_ABS))),
        0xd9e4 => return Ok(fcom(f64c(0.0))),
        0xd9e5 => return Ok(fxam()),
        0xd9e8 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(1.0)));
            return Ok(v);
        }
        0xd9e9 => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(std::f64::consts::LOG2_10)));
            return Ok(v);
        }
        0xd9ea => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(std::f64::consts::LOG2_E)));
            return Ok(v);
        }
        0xd9eb => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(std::f64::consts::PI)));
            return Ok(v);
        }
        0xd9ec => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(std::f64::consts::LOG10_2)));
            return Ok(v);
        }
        0xd9ed => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(std::f64::consts::LN_2)));
            return Ok(v);
        }
        0xd9ee => {
            let mut v = push_top();
            v.extend(store_f64(slot(0), f64c(0.0)));
            return Ok(v);
        }
        0xd9f6 => {
            let mut v = reg(abi::FP_TOP);
            v.extend(i32c(7));
            v.push(Op::I32_ADD);
            v.extend(i32c(7));
            v.push(Op::I32_AND);
            return Ok(set_reg(abi::FP_TOP, v)); // FDECSTP
        }
        0xd9f7 => return Ok(pop_top()), // FINCSTP
        0xd9fa => return Ok(set_st0(unop_st0(Op::F64_SQRT))),
        0xd9fc => return Ok(set_st0(round_integer(load_f64(slot(0))))),
        0xd9fd => return Ok(fscale()),
        0xd9f8 => return Ok(fprem(false)),
        0xd9f5 => return Ok(fprem(true)),
        0xd9f0 => return Ok(helper(abi::fpu_helper::F2XM1)),
        0xd9f1 => return Ok(helper(abi::fpu_helper::FYL2X)),
        0xd9f2 => return Ok(helper(abi::fpu_helper::FPTAN)),
        0xd9f3 => return Ok(helper(abi::fpu_helper::FPATAN)),
        0xd9f9 => return Ok(helper(abi::fpu_helper::FYL2XP1)),
        0xd9fb => return Ok(helper(abi::fpu_helper::FSINCOS)),
        0xd9fe => return Ok(helper(abi::fpu_helper::FSIN)),
        0xd9ff => return Ok(helper(abi::fpu_helper::FCOS)),
        _ => {}
    }
    if raw == 0xdfe0 {
        // FNSTSW AX
        let mut v = reg(abi::EAX);
        v.extend(i32c(0xffff_0000u32 as i32));
        v.push(Op::I32_AND);
        v.extend(status_word());
        v.push(Op::I32_OR);
        return Ok(set_reg(abi::EAX, v));
    }
    if raw == 0xdbe2 {
        let mut status = reg(abi::FP_STATUS);
        status.extend(i32c(!0x80ff)); // FNCLEX: clear exceptions, stack fault, ES and busy.
        status.push(Op::I32_AND);
        return Ok(set_reg(abi::FP_STATUS, status));
    }
    if raw == 0xdbe3 {
        let mut v = set_reg(abi::FP_TOP, i32c(0)); // FNINIT
        v.extend(set_reg(abi::FP_STATUS, i32c(0)));
        v.extend(set_reg(abi::FP_CONTROL, i32c(CONTROL_WORD)));
        return Ok(v);
    }
    Err(format!("FPU opcode {raw:#x} unsupported"))
}
