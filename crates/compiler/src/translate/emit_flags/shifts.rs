//! Carry, overflow and result flags for shifts and rotations.

use super::{
    abi, i32c, local_get, local_set, pf_seq, pick, reg, set_flags, sleb, zf_seq, zs_pf, Op, Seq, A,
    B, FLAGS_G, RES, T1, T2, T3,
};

/// Narrow shifts use the operand's sign/carry bits, not i32 bit 31.
/// At most 31 one-bit steps also handle rotate-through-carry and counts wider
/// than the operand without relying on Wasm's modulo-32 shift semantics.
pub fn flag_narrow_shift(kind: Kind, bits: u32) -> Seq {
    let mask = (u32::MAX >> (32 - bits)) as i32;
    let sign = 1i32 << (bits - 1);
    let left = matches!(kind, "shl" | "rol" | "rcl");
    let rotate = matches!(kind, "rol" | "ror" | "rcl" | "rcr");
    let mut v = local_get(A);
    v.extend(local_set(RES));
    v.extend(cnt_seq());
    v.extend(local_set(B));
    v.extend(local_get(B));
    v.push(Op::IF);
    v.push(0x40);
    v.extend(local_get(B));
    v.extend(local_set(T1));
    v.extend(reg(abi::FLAGS));
    v.extend(i32c(1));
    v.push(Op::I32_AND);
    v.extend(local_set(T2));
    v.push(Op::LOOP);
    v.push(0x40);
    v.extend(local_get(T2));
    v.extend(local_set(T3)); // carry input for this step
    v.extend(local_get(RES));
    if left {
        v.extend(i32c(bits as i32 - 1));
        v.push(Op::I32_SHR_U);
    }
    v.extend(i32c(1));
    v.push(Op::I32_AND);
    v.extend(local_set(T2));
    v.extend(local_get(RES));
    v.extend(i32c(1));
    v.push(if left { Op::I32_SHL } else { Op::I32_SHR_U });
    match kind {
        "rol" => {
            v.extend(local_get(T2));
            v.push(Op::I32_OR);
        }
        "rcl" => {
            v.extend(local_get(T3));
            v.push(Op::I32_OR);
        }
        "ror" | "rcr" => {
            v.extend(local_get(if kind == "ror" { T2 } else { T3 }));
            v.extend(i32c(bits as i32 - 1));
            v.push(Op::I32_SHL);
            v.push(Op::I32_OR);
        }
        "sar" => {
            v.extend(local_get(RES));
            v.extend(i32c(sign));
            v.push(Op::I32_AND);
            v.push(Op::I32_OR);
        }
        _ => {}
    }
    v.extend(i32c(mask));
    v.push(Op::I32_AND);
    v.extend(local_set(RES));
    v.extend(local_get(T1));
    v.extend(i32c(1));
    v.push(Op::I32_SUB);
    v.push(Op::LOCAL_TEE);
    v.extend(sleb(T1 as i32));
    v.push(Op::BR_IF);
    v.push(0);
    v.push(Op::END);
    // Rotates preserve SF/ZF/PF; shifts derive them from the narrowed result.
    let changed = abi::CF
        | if rotate {
            0
        } else {
            abi::SF | abi::ZF | abi::PF
        };
    v.extend(reg(abi::FLAGS));
    v.extend(i32c(!changed));
    v.push(Op::I32_AND);
    v.extend(local_get(T2));
    v.push(Op::I32_OR);
    if !rotate {
        v.extend(zf_seq());
        v.push(Op::I32_OR);
        v.extend(local_get(RES));
        v.extend(i32c(sign));
        v.push(Op::I32_AND);
        v.extend(i32c(bits as i32 - 8));
        v.push(Op::I32_SHR_U);
        v.push(Op::I32_OR);
        v.extend(pf_seq());
        v.push(Op::I32_OR);
    }
    v.extend(local_set(T3));
    v.extend(local_get(B));
    v.extend(i32c(1));
    v.push(Op::I32_EQ);
    v.push(Op::IF);
    v.push(0x40);
    v.extend(local_get(T3));
    v.extend(i32c(!abi::OF));
    v.push(Op::I32_AND);
    match kind {
        "sar" => v.extend(i32c(0)),
        "shr" => {
            v.extend(local_get(A));
            v.extend(i32c(bits as i32 - 1));
            v.push(Op::I32_SHR_U);
        }
        "ror" | "rcr" => {
            v.extend(local_get(RES));
            v.extend(i32c(bits as i32 - 1));
            v.push(Op::I32_SHR_U);
            v.extend(local_get(RES));
            v.extend(i32c(bits as i32 - 2));
            v.push(Op::I32_SHR_U);
            v.push(Op::I32_XOR);
            v.extend(i32c(1));
            v.push(Op::I32_AND);
        }
        _ => {
            v.extend(local_get(RES));
            v.extend(i32c(bits as i32 - 1));
            v.push(Op::I32_SHR_U);
            v.extend(local_get(T2));
            v.push(Op::I32_XOR);
        }
    }
    v.extend(i32c(11));
    v.push(Op::I32_SHL);
    v.push(Op::I32_OR);
    v.extend(local_set(T3));
    v.push(Op::END);
    v.extend(set_flags(local_get(T3)));
    v.push(Op::END);
    v
}

/// Shift/rotate; B holds the count. Guarded: count 0 changes nothing.
pub fn flag_shift(kind: Kind) -> Seq {
    let mut v = cnt_seq();
    v.push(Op::IF);
    v.push(0x40);
    v.extend(shift_body(kind));
    v.push(Op::END);
    v
}

pub type Kind = &'static str;

pub(super) fn cnt_seq() -> Seq {
    let mut v = local_get(B);
    v.extend(i32c(31));
    v.push(Op::I32_AND);
    v
}

pub(super) fn c_is_one() -> Seq {
    let mut v = cnt_seq();
    v.extend(i32c(1));
    v.push(Op::I32_EQ);
    v
}

pub(super) fn compose() -> Seq {
    vec![Op::I32_OR, Op::I32_OR, Op::GLOBAL_SET, FLAGS_G as u8]
}

pub(super) fn shift_body(kind: Kind) -> Seq {
    match kind {
        "shl" => {
            let mut v = set_res(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.push(Op::I32_SHL);
            });
            v.extend(zs_pf());
            v.extend(cf_bit(|v| {
                v.extend(local_get(A));
                v.extend(i32c(32));
                v.extend(cnt_seq());
                v.push(Op::I32_SUB);
                v.push(Op::I32_SHR_U);
                v.extend(i32c(1));
                v.push(Op::I32_AND);
            }));
            v.extend(pick(i32c(abi::OF), i32c(0), {
                let mut x = local_get(RES);
                x.extend(i32c(31));
                x.push(Op::I32_SHR_U);
                x.extend(c_is_one());
                x.push(Op::I32_XOR);
                x
            }));
            v.extend(compose());
            v
        }
        "shr" => {
            let mut v = set_res(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.push(Op::I32_SHR_U);
            });
            v.extend(zs_pf());
            v.extend(cf_bit(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.extend(i32c(1));
                v.push(Op::I32_SUB);
                v.push(Op::I32_SHR_U);
                v.extend(i32c(1));
                v.push(Op::I32_AND);
            }));
            v.extend(pick(
                {
                    let mut x = local_get(A);
                    x.extend(i32c(31));
                    x.push(Op::I32_SHR_U);
                    x.extend(i32c(abi::OF));
                    x.push(Op::I32_MUL);
                    x
                },
                i32c(0),
                c_is_one(),
            ));
            v.extend(compose());
            v
        }
        "sar" => {
            let mut v = set_res(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.push(Op::I32_SHR_S);
            });
            v.extend(zs_pf());
            v.extend(cf_bit(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.extend(i32c(1));
                v.push(Op::I32_SUB);
                v.push(Op::I32_SHR_U);
                v.extend(i32c(1));
                v.push(Op::I32_AND);
            }));
            v.extend(i32c(0)); // SAR clears OF
            v.extend(compose());
            v
        }
        "rol" => {
            let mut v = set_res(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.push(Op::I32_ROTL);
            });
            // Rotates preserve all flags except CF/OF.
            v.extend(reg(FLAGS_G as u8));
            v.extend(i32c(!(abi::CF | abi::OF)));
            v.push(Op::I32_AND);
            v.extend(cf_bit(|v| {
                v.extend(local_get(RES));
                v.extend(i32c(1));
                v.push(Op::I32_AND);
            }));
            v.extend(pick(i32c(abi::OF), i32c(0), {
                let mut x = local_get(RES);
                x.extend(i32c(31));
                x.push(Op::I32_SHR_U);
                x.extend(local_get(RES));
                x.extend(i32c(1));
                x.push(Op::I32_AND);
                x.push(Op::I32_XOR);
                x.extend(c_is_one());
                x.push(Op::I32_AND);
                x
            }));
            v.extend(compose());
            v
        }
        "ror" => {
            let mut v = set_res(|v| {
                v.extend(local_get(A));
                v.extend(cnt_seq());
                v.push(Op::I32_ROTR);
            });
            // Rotates preserve all flags except CF/OF.
            v.extend(reg(FLAGS_G as u8));
            v.extend(i32c(!(abi::CF | abi::OF)));
            v.push(Op::I32_AND);
            v.extend(cf_bit(|v| {
                v.extend(local_get(RES));
                v.extend(i32c(31));
                v.push(Op::I32_SHR_U);
            }));
            v.extend(pick(i32c(abi::OF), i32c(0), {
                let mut x = local_get(RES);
                x.extend(i32c(31));
                x.push(Op::I32_SHR_U);
                x.extend(local_get(RES));
                x.extend(i32c(30));
                x.push(Op::I32_SHR_U);
                x.extend(i32c(1));
                x.push(Op::I32_AND);
                x.push(Op::I32_XOR);
                x.extend(c_is_one());
                x.push(Op::I32_AND);
                x
            }));
            v.extend(compose());
            v
        }
        "rcl" => rot_through_carry(true),
        "rcr" => rot_through_carry(false),
        _ => panic!("bad shift {}", kind),
    }
}

pub(super) fn set_res(f: impl FnOnce(&mut Seq)) -> Seq {
    let mut v = Seq::new();
    f(&mut v);
    v.extend(local_set(RES));
    v
}

pub(super) fn cf_bit(f: impl FnOnce(&mut Seq)) -> Seq {
    let mut v = Seq::new();
    f(&mut v);
    v.extend(i32c(abi::CF));
    v.push(Op::I32_MUL);
    v
}

/// Rotate through carry using the same bounded one-bit steps as narrow operands.
/// Keeping carry separate avoids Wasm's modulo-width shift behavior and preserves
/// every flag except CF (and OF when the masked count is one).
pub(super) fn rot_through_carry(left: bool) -> Seq {
    flag_narrow_shift(if left { "rcl" } else { "rcr" }, 32)
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/emit_flags/shifts.rs"]
mod tests;
