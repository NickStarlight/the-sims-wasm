//! Condition-code test → i32 (0/1) from the FLAGS global — mirrors src/translate/cond.ts.

use super::emit_helpers::{i32c, reg, Seq};
use crate::wasm::opcodes::Op;

fn bit(shift: i32) -> Seq {
    let mut v = reg(8); // FLAGS global
    v.extend(i32c(shift));
    v.push(Op::I32_SHR_U);
    v.extend(i32c(1));
    v.push(Op::I32_AND);
    v
}

fn not(c: Seq) -> Seq {
    let mut v = c;
    v.extend(i32c(1));
    v.push(Op::I32_XOR);
    v
}

pub fn emit_cond(cc: u8) -> Seq {
    match cc {
        0 => bit(11),      // O
        1 => not(bit(11)), // NO
        2 => bit(0),       // B
        3 => not(bit(0)),  // AE
        4 => bit(6),       // E
        5 => not(bit(6)),  // NE
        6 => {
            let mut v = bit(0); // BE
            v.extend(bit(6));
            v.push(Op::I32_OR);
            v
        }
        7 => {
            let mut v = bit(0); // A
            v.extend(bit(6));
            v.push(Op::I32_OR);
            not(v)
        }
        8 => bit(7),       // S
        9 => not(bit(7)),  // NS
        10 => bit(2),      // P
        11 => not(bit(2)), // NP
        12 => {
            let mut v = bit(7); // L
            v.extend(bit(11));
            v.push(Op::I32_XOR);
            v
        }
        13 => {
            let mut v = bit(7); // GE
            v.extend(bit(11));
            v.push(Op::I32_XOR);
            not(v)
        }
        14 => {
            let mut v = bit(6); // LE
            v.extend(bit(7));
            v.extend(bit(11));
            v.push(Op::I32_XOR);
            v.push(Op::I32_OR);
            v
        }
        15 => {
            let mut v = bit(6); // G
            v.extend(bit(7));
            v.extend(bit(11));
            v.push(Op::I32_XOR);
            v.push(Op::I32_OR);
            not(v)
        }
        _ => panic!("bad cc {}", cc),
    }
}
