//! Byte and word arithmetic flags.
use super::{
    abi, i32c, local_get, local_set, pf_seq, pick, reg, sleb, zf_seq, Op, Seq, A, B, FLAGS_G, RES,
};

/// Width-correct flags for byte/word arithmetic and logic. A/B/RES are unsigned
/// operands and the untruncated result; keep unrelated architectural flags.
pub fn flag_narrow_alu(mnem: crate::x86::types::Mnem, size: u8) -> Seq {
    use crate::x86::types::Mnem::*;
    let mask = if size == 1 { 0xff } else { 0xffff };
    let sign = (mask + 1) >> 1;
    let logic = matches!(mnem, And | Or | Xor | Test);
    let add = matches!(mnem, Add | Adc | Inc);
    let keep_cf = matches!(mnem, Inc | Dec);
    let mut out = Vec::new();
    for slot in [A, B, RES] {
        out.extend(local_get(slot));
        out.extend(i32c(mask));
        out.push(Op::I32_AND);
        out.extend(local_set(slot));
    }
    let changed = abi::ZF
        | abi::SF
        | abi::PF
        | abi::OF
        | if keep_cf { 0 } else { abi::CF }
        | if logic { 0 } else { 0x10 };
    out.extend(reg(abi::FLAGS));
    out.extend(i32c(!changed));
    out.push(Op::I32_AND);
    out.extend(zf_seq());
    out.push(Op::I32_OR);
    out.extend(pf_seq());
    out.push(Op::I32_OR);
    let mut negative = local_get(RES);
    negative.extend(i32c(sign));
    negative.push(Op::I32_AND);
    out.extend(pick(i32c(abi::SF), i32c(0), negative));
    out.push(Op::I32_OR);
    if !logic {
        if !keep_cf {
            let mut carry = local_get(A);
            if add {
                carry.extend(local_get(B));
                carry.push(Op::I32_ADD);
                if mnem == Adc {
                    carry.extend(reg(abi::FLAGS));
                    carry.extend(i32c(abi::CF));
                    carry.push(Op::I32_AND);
                    carry.push(Op::I32_ADD);
                }
                carry.extend(i32c(mask));
                carry.push(Op::I32_GT_U);
            } else {
                carry.extend(local_get(B));
                if mnem == Sbb {
                    carry.extend(reg(abi::FLAGS));
                    carry.extend(i32c(abi::CF));
                    carry.push(Op::I32_AND);
                    carry.push(Op::I32_ADD);
                }
                carry.push(Op::I32_LT_U);
            }
            out.extend(pick(i32c(abi::CF), i32c(0), carry));
            out.push(Op::I32_OR);
        }
        let mut overflow = local_get(A);
        overflow.extend(local_get(B));
        overflow.push(Op::I32_XOR);
        if add {
            overflow.extend(i32c(-1));
            overflow.push(Op::I32_XOR);
        }
        overflow.extend(local_get(A));
        overflow.extend(local_get(RES));
        overflow.push(Op::I32_XOR);
        overflow.push(Op::I32_AND);
        overflow.extend(i32c(sign));
        overflow.push(Op::I32_AND);
        out.extend(pick(i32c(abi::OF), i32c(0), overflow));
        out.push(Op::I32_OR);
        out.extend(local_get(A));
        out.extend(local_get(B));
        out.push(Op::I32_XOR);
        out.extend(local_get(RES));
        out.push(Op::I32_XOR);
        out.extend(i32c(0x10));
        out.push(Op::I32_AND);
        out.push(Op::I32_OR);
    }
    out.extend(vec![Op::GLOBAL_SET]);
    out.extend(sleb(FLAGS_G));
    out
}
