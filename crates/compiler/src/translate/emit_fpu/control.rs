//! x87 control-word rounding for FRNDINT and integer stores.
use super::{abi, i32c, local_get, local_set, reg, Op, Seq, FTMP};

pub(super) fn round_integer(value: Seq) -> Seq {
    let mut out = value;
    out.extend(local_set(FTMP));
    for (mode, opcode) in [
        (0x400, Op::F64_FLOOR),
        (0x800, Op::F64_CEIL),
        (0xc00, Op::F64_TRUNC),
    ] {
        out.extend(reg(abi::FP_CONTROL));
        out.extend(i32c(0xc00));
        out.push(Op::I32_AND);
        out.extend(i32c(mode));
        out.push(Op::I32_EQ);
        out.extend([Op::IF, 0x7c]); // f64 result
        out.extend(local_get(FTMP));
        out.push(opcode);
        out.push(Op::ELSE);
    }
    out.extend(local_get(FTMP));
    out.push(Op::F64_NEAREST);
    out.extend([Op::END; 3]);
    out
}
