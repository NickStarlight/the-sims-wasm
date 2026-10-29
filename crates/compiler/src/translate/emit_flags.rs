//! Eager EFLAGS computation for translated integer instructions.
//! Operands a/b/result must already sit in locals A/B/RES.

use super::abi;
use super::emit_helpers::{i32c, reg, Seq};
use super::emit_helpers::{local_get, local_set};
use super::layout::{A, B, FULL, RES, T1, T2, T3};
use crate::wasm::leb::sleb;
use crate::wasm::opcodes::Op;

const FLAGS_G: i32 = 8;

/// select: [v1, v2, cond] → cond ? v1 : v2
fn pick(v1: Seq, v2: Seq, cond: Seq) -> Seq {
    let mut v = v1;
    v.extend(v2);
    v.extend(cond);
    v.push(Op::SELECT);
    v
}

fn zf_seq() -> Seq {
    pick(i32c(abi::ZF), i32c(0), {
        let mut v = local_get(RES);
        v.push(Op::I32_EQZ);
        v
    })
}

fn sf_seq() -> Seq {
    pick(i32c(abi::SF), i32c(0), {
        let mut v = local_get(RES);
        v.extend(i32c(0));
        v.push(Op::I32_LT_S);
        v
    })
}

fn pf_seq() -> Seq {
    let mut v = local_get(RES);
    v.extend(i32c(255));
    v.push(Op::I32_AND);
    v.push(Op::I32_POPCNT);
    v.extend(i32c(1));
    v.push(Op::I32_AND);
    v.extend(i32c(1));
    v.push(Op::I32_XOR);
    v.extend(i32c(abi::PF));
    v.push(Op::I32_MUL);
    v
}

fn zs_pf() -> Seq {
    let mut v = zf_seq();
    v.extend(sf_seq());
    v.extend(pf_seq());
    v.push(Op::I32_OR);
    v.push(Op::I32_OR);
    v
}

fn set_flags(mut v: Seq) -> Seq {
    v.push(Op::GLOBAL_SET);
    v.extend(sleb(FLAGS_G));
    v
}

/// and/or/xor/test: CF=0, OF=0.
pub fn flag_logic() -> Seq {
    set_flags(zs_pf())
}

mod shifts;
pub use shifts::{flag_narrow_shift, flag_shift, Kind};

mod narrow;
pub use narrow::flag_narrow_alu;

mod arithmetic;
pub use arithmetic::{
    flag_add, flag_add_carry, flag_inc_dec, flag_mul, flag_neg, flag_sh_double, flag_sub,
    flag_sub_borrow,
};

#[cfg(test)]
#[path = "../../tests/unit/translate/emit_flags.rs"]
mod tests;
