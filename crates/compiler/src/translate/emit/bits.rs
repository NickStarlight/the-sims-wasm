//! Bit tests, scans and byte swapping.

use super::{
    abi, emit_address, i32c, local_get, local_set, pick, read_op, reg, set_reg, stash, write_op,
    Op, Operand, Seq, A, B, T1,
};

/// BT family: CF = bit(dest, count); optionally modify the bit.
pub(super) fn emit_bt(dest: &Operand, bit: &Operand, modify: bool, op: u8, invert: bool) -> Seq {
    let mask = || {
        let mut v = i32c(1);
        v.extend(local_get(B));
        v.extend(i32c(31));
        v.push(Op::I32_AND);
        v.push(Op::I32_SHL);
        v
    };
    if let Operand::Mem(m) = dest {
        if let Operand::Reg { .. } = bit {
            // memory dest + register bit offset: dword address = dest + 4*(idx>>5)
            let dword_addr = {
                let mut v = emit_address(m);
                v.extend(local_get(B));
                v.extend(i32c(5));
                v.push(Op::I32_SHR_U);
                v.extend(i32c(2));
                v.push(Op::I32_SHL);
                v.push(Op::I32_ADD);
                v.extend(local_set(T1));
                v
            };
            let bit_in_dword = {
                let mut v = local_get(B);
                v.extend(i32c(31));
                v.push(Op::I32_AND);
                v
            };
            let cf_val = {
                let mut v = i32c(abi::CF);
                v.extend(i32c(0));
                let cond = {
                    let mut c = local_get(T1);
                    c.push(Op::I32_LOAD);
                    c.push(2);
                    c.push(0);
                    c.extend(bit_in_dword);
                    c.push(Op::I32_SHR_U);
                    c.extend(i32c(1));
                    c.push(Op::I32_AND);
                    c
                };
                v.extend(cond);
                v.push(Op::SELECT);
                v
            };
            let mut seq = stash(bit, B);
            seq.extend(dword_addr);
            seq.extend(cf_val);
            let mut fl = reg(abi::FLAGS);
            fl.extend(i32c(!abi::CF));
            fl.push(Op::I32_AND);
            fl.push(Op::I32_OR);
            seq.extend(set_reg(abi::FLAGS, fl));
            if !modify {
                return seq;
            }
            let flipped = if invert {
                let mut v = mask();
                v.extend(i32c(-1));
                v.push(Op::I32_XOR);
                v
            } else {
                mask()
            };
            // [addr(T1), value(load(T1) flipped), i32.store]
            seq.extend(local_get(T1));
            seq.extend(local_get(T1));
            seq.push(Op::I32_LOAD);
            seq.push(2);
            seq.push(0);
            seq.extend(flipped);
            seq.push(op);
            seq.push(Op::I32_STORE);
            seq.push(2);
            seq.push(0);
            return seq;
        }
    }
    let mut seq = stash(dest, A);
    seq.extend(stash(bit, B));
    let cf_val = {
        let mut v = i32c(abi::CF);
        v.extend(i32c(0));
        let cond = {
            let mut c = local_get(A);
            c.extend(local_get(B));
            c.extend(i32c(31));
            c.push(Op::I32_AND);
            c.push(Op::I32_SHR_U);
            c.extend(i32c(1));
            c.push(Op::I32_AND);
            c
        };
        v.extend(cond);
        v.push(Op::SELECT);
        v
    };
    seq.extend(cf_val);
    let mut fl = reg(abi::FLAGS);
    fl.extend(i32c(!abi::CF));
    fl.push(Op::I32_AND);
    fl.push(Op::I32_OR);
    seq.extend(set_reg(abi::FLAGS, fl));
    if !modify {
        return seq;
    }
    let flipped = if invert {
        let mut v = mask();
        v.extend(i32c(-1));
        v.push(Op::I32_XOR);
        v
    } else {
        mask()
    };
    let mut val = local_get(A);
    val.extend(flipped);
    val.push(op);
    seq.extend(write_op(dest, val));
    seq
}

/// BSF/BSR: dest = bit index (unchanged when src==0); ZF = src==0.
pub(super) fn emit_bs(forward: bool, dest: &Operand, src: &Operand) -> Seq {
    let index: Seq = if forward {
        let mut v = local_get(A);
        v.push(Op::I32_CTZ);
        v
    } else {
        let mut v = local_get(A);
        v.push(Op::I32_CLZ);
        v.extend(i32c(31));
        v.push(Op::I32_XOR);
        v
    };
    let mut out = stash(src, A);
    let mut fl = i32c(0x40);
    fl.extend(i32c(0));
    fl.extend(local_get(A));
    fl.push(Op::I32_EQZ);
    fl.push(Op::SELECT);
    out.extend(set_reg(abi::FLAGS, fl));
    let val = pick(read_op(dest), index, {
        let mut c = local_get(A);
        c.push(Op::I32_EQZ);
        c
    });
    out.extend(write_op(dest, val));
    out
}

/// BSWAP r32.
pub(super) fn emit_bswap(dest: &Operand) -> Seq {
    let mut out = stash(dest, A);
    out.extend(local_get(A));
    out.extend(i32c(0xff));
    out.push(Op::I32_AND);
    out.extend(i32c(24));
    out.push(Op::I32_SHL);
    out.extend(local_get(A));
    out.extend(i32c(8));
    out.push(Op::I32_SHL);
    out.extend(i32c(0xff0000));
    out.push(Op::I32_AND);
    out.push(Op::I32_OR);
    out.extend(local_get(A));
    out.extend(i32c(8));
    out.push(Op::I32_SHR_U);
    out.extend(i32c(0xff00));
    out.push(Op::I32_AND);
    out.push(Op::I32_OR);
    out.extend(local_get(A));
    out.extend(i32c(24));
    out.push(Op::I32_SHR_U);
    out.push(Op::I32_OR);
    out.extend(write_op(dest, Vec::new()));
    out
}

#[cfg(test)]
#[path = "../../../tests/unit/translate/emit/bits.rs"]
mod tests;
