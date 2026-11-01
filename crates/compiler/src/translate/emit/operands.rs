//! Read/write x86 operands and construct local control flow.

use super::{
    emit_load, emit_store, i32c, local_set, reg, set_reg, size_mask, sleb, MemOp, Op, Operand, Reg,
    Seq, NEXT,
};

pub(super) fn mem_at(base: u8, disp: i32, size: u8) -> MemOp {
    MemOp {
        base: base as i8,
        index: -1,
        scale: 1,
        disp,
        size,
        fs: false,
    }
}

/// Byte-register r 0-7: 0-3 = low bytes of globals 0-3, 4-7 = HIGH bytes (bits 8-15).
pub(super) fn byte_reg(r: Reg) -> (Reg, i32) {
    if r < 4 {
        (r, 0)
    } else {
        (r - 4, 8)
    }
}

/// Operand read → i32 code.
pub(super) fn read_op(o: &Operand) -> Seq {
    match o {
        Operand::Reg { r, size } => {
            if *size == 4 {
                return reg(*r);
            }
            if *size == 1 && *r >= 4 {
                let (g, shift) = byte_reg(*r);
                let mut v = reg(g);
                v.extend(i32c(shift));
                v.push(Op::I32_SHR_U);
                v.extend(i32c(0xff));
                v.push(Op::I32_AND);
                return v;
            }
            let mut v = reg(*r);
            v.extend(size_mask(*size as u32));
            v
        }
        Operand::Imm(v) => i32c(*v),
        Operand::Mem(m) => emit_load(m),
        Operand::St(_) => panic!("st operand outside fpu"),
    }
}

pub(super) fn write_op(o: &Operand, value: Seq) -> Seq {
    match o {
        Operand::Reg { r, size } => {
            if *size == 4 {
                return set_reg(*r, value);
            }
            // byte/word write: preserve the untouched bits of the 32-bit reg
            let m: i32;
            let v: Seq;
            if *size == 1 && *r >= 4 {
                m = 0xff00;
                let mut vv = value;
                vv.extend(i32c(8));
                vv.push(Op::I32_SHL);
                v = vv;
            } else {
                m = if *size == 1 { 0xff } else { 0xffff };
                v = value;
            }
            let target = if *size == 1 && *r >= 4 { r - 4 } else { *r };
            let mut out = reg(target);
            out.extend(i32c(!m));
            out.push(Op::I32_AND);
            out.extend(v);
            out.extend(i32c(m));
            out.push(Op::I32_AND);
            out.push(Op::I32_OR);
            set_reg(target, out)
        }
        Operand::Mem(m) => emit_store(m, value),
        _ => panic!("bad write operand"),
    }
}

pub(super) fn stash(o: &Operand, slot: u32) -> Seq {
    let mut v = read_op(o);
    v.extend(local_set(slot));
    v
}

/// Continue to a dispatch id.
pub(super) fn to(id: i32, br_depth: i32) -> Seq {
    let mut v = i32c(id);
    v.extend(local_set(NEXT));
    v.push(Op::BR);
    v.extend(sleb(br_depth));
    v
}

pub(super) fn pick(v1: Seq, v2: Seq, cond: Seq) -> Seq {
    let mut v = v1;
    v.extend(v2);
    v.extend(cond);
    v.push(Op::SELECT);
    v
}
