//! x87 translation for the Rust AOT compiler.
//! Register file lives in guest memory (dynamic st(i) indexing).

use super::abi;
use super::emit_helpers::{emit_address, emit_load, i32c, i64c, reg, set_reg, Seq};
use super::emit_helpers::{local_get, local_set};
use super::layout::{FTMP, FULL, T1};
use crate::wasm::leb::sleb;
use crate::wasm::opcodes::Op;
use crate::x86::types::{Insn, MemOp};

pub const C0: i32 = 0x0100;
pub const C2: i32 = 0x0400;
pub const C3: i32 = 0x4000;

/// Address code for the f64 slot of st(i). st(i) = physical register (top + i).
fn slot(i: i32) -> Seq {
    let mut v = reg(abi::FP_TOP);
    v.extend(i32c(i));
    v.push(Op::I32_ADD);
    v.extend(i32c(7));
    v.push(Op::I32_AND);
    v.extend(i32c(8));
    v.push(Op::I32_MUL);
    v.extend(reg(abi::TEB_BASE));
    v.push(Op::I32_ADD);
    v.extend(i32c(abi::FP_OFFSET));
    v.push(Op::I32_ADD);
    v
}

fn push_top() -> Seq {
    let mut v = reg(abi::FP_TOP);
    v.extend(i32c(7));
    v.push(Op::I32_ADD);
    v.extend(i32c(7));
    v.push(Op::I32_AND);
    set_reg(abi::FP_TOP, v)
}

fn pop_top() -> Seq {
    let mut v = reg(abi::FP_TOP);
    v.extend(i32c(1));
    v.push(Op::I32_ADD);
    v.extend(i32c(7));
    v.push(Op::I32_AND);
    set_reg(abi::FP_TOP, v)
}

fn load_f64(addr: Seq) -> Seq {
    let mut v = addr;
    v.push(Op::F64_LOAD);
    v.push(3);
    v.push(0);
    v
}

fn store_f64(addr: Seq, value: Seq) -> Seq {
    let mut v = addr;
    v.extend(value);
    v.push(Op::F64_STORE);
    v.push(3);
    v.push(0);
    v
}

fn set_st0(value: Seq) -> Seq {
    store_f64(slot(0), value)
}

/// Load a memory operand as f64 (m32 promotes, m64 direct, ints convert).
fn read_f64(m: &MemOp, kind: Kind) -> Seq {
    match kind {
        "f64" => load_f64(emit_address(m)),
        "f32" => {
            let mut v = emit_address(m);
            v.push(Op::F32_LOAD);
            v.push(0x02); // memarg(align=4B, offset=0)
            v.push(0x00);
            v.push(Op::F64_PROMOTE_F32);
            v
        }
        "i32" => {
            let mut mm = m.clone();
            mm.size = 4;
            let mut v = emit_load(&mm);
            v.push(Op::F64_CONVERT_I32_S);
            v
        }
        "i16" => {
            let mut v = emit_address(m);
            v.push(Op::I32_LOAD16_S);
            v.push(1);
            v.push(0);
            v.push(Op::F64_CONVERT_I32_S);
            v
        }
        _ => unreachable!(),
    }
}
type Kind = &'static str;

fn f64_bytes(v: f64) -> [u8; 8] {
    v.to_le_bytes()
}

fn f64c(v: f64) -> Seq {
    let mut out = vec![Op::F64_CONST];
    out.extend(f64_bytes(v));
    out
}

/// Compare st0 vs value → FP_STATUS (C0/C2/C3).
fn fcom(value: Seq) -> Seq {
    let a = load_f64(slot(0));
    let unord = {
        let mut v = a.clone();
        v.extend(a.clone());
        v.push(Op::F64_NE);
        v
    };
    let lt = {
        let mut v = a.clone();
        v.extend(value.clone());
        v.push(Op::F64_LT);
        v
    };
    let eq = {
        let mut v = a;
        v.extend(value);
        v.push(Op::F64_EQ);
        v
    };
    set_reg(abi::FP_STATUS, {
        let mut v = reg(abi::FP_STATUS);
        v.extend(i32c(!(C0 | C1 | C2 | C3)));
        v.push(Op::I32_AND);
        v.extend(sel(i32c(C0 | C2 | C3), i32c(0), unord));
        v.push(Op::I32_OR);
        v.extend(sel(i32c(C0), i32c(0), lt));
        v.push(Op::I32_OR);
        v.extend(sel(i32c(C3), i32c(0), eq));
        v.push(Op::I32_OR);
        v
    })
}

fn sel(v1: Seq, v2: Seq, cond: Seq) -> Seq {
    let mut v = v1;
    v.extend(v2);
    v.extend(cond);
    v.push(Op::SELECT);
    v
}

/// Translate one FPU instruction; ops[0] = packed raw opcode, ops[1] = st(i) or mem.
pub fn emit_fpu(insn: &Insn) -> Result<Seq, super::error::EmitError> {
    emit_fpu_inner(insn).map_err(|message| super::error::EmitError::new(insn.va, message))
}

fn emit_fpu_inner(insn: &Insn) -> Result<Seq, String> {
    let raw = match insn.ops.first() {
        Some(crate::x86::types::Operand::Imm(v)) => *v as u32,
        _ => return Err("FPU instruction requires a raw opcode".into()),
    };
    let row = raw >> 8;
    let modrm = raw & 0xff;
    let mod_ = modrm >> 6;
    let reg_field = ((modrm >> 3) & 7) as u8;
    let operand = insn.ops.get(1);

    if mod_ == 3 {
        let i = match operand {
            Some(crate::x86::types::Operand::St(i)) => *i,
            _ => 0,
        };
        return emit_fpu_reg(raw, reg_field, i);
    }

    let m = match operand {
        Some(crate::x86::types::Operand::Mem(m)) => m,
        _ => return Err("FPU memory form requires a memory operand".into()),
    };
    match row {
        0xd8 => Ok(float_arith(reg_field, read_f64(m, "f32"))),
        0xdc => Ok(float_arith(reg_field, read_f64(m, "f64"))),
        0xda => Ok(int_arith(reg_field, read_f64(m, "i32"))),
        0xde => Ok(int_arith(reg_field, read_f64(m, "i16"))),
        0xd9 => d9mem(reg_field, m),
        0xdb => dbmem(reg_field, m),
        0xdd => ddmem(reg_field, m),
        0xdf => dfmem(reg_field, m),
        _ => Err(format!("FPU row {row:#x} unsupported")),
    }
}

/// D8/DC memory arithmetic: FADD/FMUL/FCOM/FCOMP/FSUB/FSUBR/FDIV/FDIVR.
fn float_arith(reg_field: u8, value: Seq) -> Seq {
    match reg_field {
        0 => bin_st0(Op::F64_ADD, value),
        1 => bin_st0(Op::F64_MUL, value),
        2 => fcom(value),
        3 => {
            let mut v = fcom(value);
            v.extend(pop_top());
            v
        }
        4 => bin_st0(Op::F64_SUB, value),
        5 => {
            let mut v = value;
            v.extend(load_f64(slot(0)));
            v.push(Op::F64_SUB);
            set_st0(v)
        }
        6 => bin_st0(Op::F64_DIV, value),
        7 => {
            let mut v = value;
            v.extend(load_f64(slot(0)));
            v.push(Op::F64_DIV);
            set_st0(v)
        }
        _ => panic!("bad float op"),
    }
}

fn int_arith(reg_field: u8, value: Seq) -> Seq {
    float_arith(reg_field, value)
}

/// Initial x87 control word: round-nearest, 53-bit precision, all exceptions masked.
pub(crate) const CONTROL_WORD: i32 = 0x027f;
const C1: i32 = 0x0200;
fn store_f64m(m: &MemOp, wide: bool) -> Seq {
    let addr = emit_address(m);
    let value = load_f64(slot(0));
    if wide {
        let mut v = addr;
        v.extend(value);
        v.push(Op::F64_STORE);
        v.push(3);
        v.push(0);
        v
    } else {
        let mut v = addr;
        v.extend(value);
        v.push(Op::F32_DEMOTE_F64);
        v.push(Op::F32_STORE);
        v.push(2);
        v.push(0);
        v
    }
}

const M52: i64 = 0x000f_ffff_ffff_ffff;

/// FIST/FISTP: convert using the active rounding mode. Masked invalid
/// conversions store the integer indefinite and set IE instead of trapping Wasm.
fn store_int(m: &MemOp, m16: bool) -> Seq {
    let mut v = emit_address(m);
    v.extend(round_integer(load_f64(slot(0))));
    v.extend(local_set(FTMP));
    let bound = if m16 { 32768.0 } else { 2147483648.0 };
    v.extend(local_get(FTMP));
    v.extend(f64c(-bound));
    v.push(Op::F64_GE);
    v.extend(local_get(FTMP));
    v.extend(f64c(bound));
    v.push(Op::F64_LT);
    v.push(Op::I32_AND); // Ordered comparisons reject NaN as well as overflow.
    v.extend([Op::IF, 0x7f]);
    v.extend(local_get(FTMP));
    v.push(Op::I32_TRUNC_F64_S);
    v.push(Op::ELSE);
    let mut status = reg(abi::FP_STATUS);
    status.extend(i32c(1));
    status.push(Op::I32_OR);
    v.extend(set_reg(abi::FP_STATUS, status));
    v.extend(i32c(if m16 { i16::MIN as i32 } else { i32::MIN }));
    v.push(Op::END);
    if m16 {
        v.push(Op::I32_STORE16);
        v.push(1);
        v.push(0);
    } else {
        v.push(Op::I32_STORE);
        v.push(2);
        v.push(0);
    }
    v
}

fn store16(m: &MemOp, value: Seq) -> Seq {
    let mut v = emit_address(m);
    v.extend(value);
    v.push(Op::I32_STORE16);
    v.push(1);
    v.push(0);
    v
}

/// Status word = C bits | top<<11.
/// FXAM examines the represented value without changing it, even NaN payloads.
/// Our f64 slots cannot retain empty tags or 80-bit unsupported/denormal encodings.
/// Nonzero binary64 subnormals are NORMAL here, as on x87 after FLD m64real:
/// their exponent is within the normal extended-precision range.
fn fxam() -> Seq {
    let mut out = slot(0);
    out.extend([Op::I64_LOAD, 3, 0]);
    out.extend(local_set(FULL));
    out.extend(reg(abi::FP_STATUS));
    out.extend(i32c(!(C0 | 0x0200 | C2 | C3)));
    out.push(Op::I32_AND);
    // Read the raw sign bit, including negative zero and negative NaNs.
    out.extend(local_get(FULL));
    out.extend(i64c(54));
    out.push(Op::I64_SHR_U);
    out.push(Op::I32_WRAP_I64);
    out.extend(i32c(0x0200));
    out.push(Op::I32_AND);
    out.push(Op::I32_OR);
    out.extend(local_get(FULL));
    out.extend(i64c(i64::MAX));
    out.push(Op::I64_AND);
    out.push(Op::I64_EQZ);
    out.extend([Op::IF, 0x7f]);
    out.extend(i32c(C3)); // zero
    out.push(Op::ELSE);
    out.extend(local_get(FULL));
    out.extend(i64c(0x7ff0_0000_0000_0000));
    out.push(Op::I64_AND);
    out.extend(i64c(0x7ff0_0000_0000_0000));
    out.push(Op::I64_EQ);
    out.extend([Op::IF, 0x7f]);
    out.extend(local_get(FULL));
    out.extend(i64c(0x000f_ffff_ffff_ffff));
    out.push(Op::I64_AND);
    out.push(Op::I64_EQZ);
    out.extend([Op::IF, 0x7f]);
    out.extend(i32c(C2 | C0)); // infinity
    out.push(Op::ELSE);
    out.extend(i32c(C0)); // NaN
    out.push(Op::END);
    out.push(Op::ELSE);
    out.extend(i32c(C2)); // normal finite
    out.extend([Op::END, Op::END, Op::I32_OR]);
    out.extend([Op::GLOBAL_SET, abi::FP_STATUS]);
    out
}

fn status_word() -> Seq {
    let mut v = reg(abi::FP_STATUS);
    v.extend(reg(abi::FP_TOP));
    v.extend(i32c(11));
    v.push(Op::I32_SHL);
    v.push(Op::I32_OR);
    v
}

mod arithmetic;
mod control;
mod environment;
mod extended;
pub(super) mod integer_copy;
mod integers;
mod memory;
mod operations;
mod registers;
pub(super) mod runs;
#[cfg(test)]
#[path = "../../tests/unit/translate/emit_fpu.rs"]
mod tests;
use arithmetic::{bin_st0, bin_st_i_pop};
use control::round_integer;
use environment::{fnsave, frstor};
use extended::{load_m80, load_m80_from, store_m80, store_m80_to};
use integers::dfmem;
use memory::{d9mem, dbmem, ddmem};
use operations::{bin_parts, fprem, fscale, fxch, helper, load_st_and_push, unop_st0};
use registers::emit_fpu_reg;
