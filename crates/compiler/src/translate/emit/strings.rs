//! String instructions and repeat-prefix loops.

use super::{
    abi, emit_store, flag_narrow_alu, flag_sub, i32c, local_get, local_set, mem_at, reg, set_reg,
    size_mask, Insn, Op, Operand, Seq, A, B, RES, T2,
};

/// REP string ops: inline loop over ECX. ops[0] = operand size in bytes.
pub(super) fn emit_string(insn: &Insn, checkpoint_import: Option<u32>) -> Seq {
    let size = match insn.ops[0] {
        Operand::Imm(v) => v as u8,
        _ => panic!("string op without size imm"),
    };
    let rep = insn.rep;
    let load = match size {
        1 => Op::I32_LOAD8_U,
        4 => Op::I32_LOAD,
        _ => Op::I32_LOAD16_U,
    };
    let mem_at_s = |r: u8| mem_at(r, 0, size);

    let load_at = |r: u8| -> Seq {
        let mut v = reg(r);
        v.push(load);
        v.push(if size == 4 {
            2
        } else if size == 2 {
            1
        } else {
            0
        });
        v.push(0);
        v
    };

    let cmp_vals = |a: Seq, b: Seq| -> Seq {
        let mut v = a;
        v.extend(local_set(A));
        v.extend(b);
        v.extend(local_set(B));
        v.extend(local_get(A));
        v.extend(local_get(B));
        v.push(Op::I32_SUB);
        v.extend(local_set(RES));
        if size < 4 {
            v.extend(flag_narrow_alu(crate::x86::types::Mnem::Cmp, size));
        } else {
            v.extend(reg(abi::FLAGS));
            v.extend(i32c(!0x8d5));
            v.push(Op::I32_AND);
            v.extend(local_set(T2));
            v.extend(flag_sub());
            let mut flags = reg(abi::FLAGS);
            flags.extend(local_get(T2));
            flags.push(Op::I32_OR);
            flags.extend(local_get(A));
            flags.extend(local_get(B));
            flags.push(Op::I32_XOR);
            flags.extend(local_get(RES));
            flags.push(Op::I32_XOR);
            flags.extend(i32c(0x10));
            flags.push(Op::I32_AND);
            flags.push(Op::I32_OR);
            v.extend(set_reg(abi::FLAGS, flags));
        }
        v
    };

    let bump = |r: u8, _sign: i32| -> Seq {
        let mut v = reg(r);
        v.extend(i32c(-(size as i32)));
        v.extend(i32c(size as i32));
        v.extend(reg(abi::FLAGS));
        v.extend(i32c(0x400)); // DF selects backward traversal, including non-REP.
        v.push(Op::I32_AND);
        v.push(Op::SELECT);
        v.push(Op::I32_ADD);
        set_reg(r, v)
    };

    let body: Seq = match insn.mnem {
        crate::x86::types::Mnem::Movs => {
            let mut v = emit_store(&mem_at_s(abi::EDI), load_at(abi::ESI));
            v.extend(bump(abi::ESI, 1));
            v.extend(bump(abi::EDI, 1));
            v
        }
        crate::x86::types::Mnem::Stos => {
            let mut v = emit_store(&mem_at_s(abi::EDI), reg(abi::EAX));
            v.extend(bump(abi::EDI, 1));
            v
        }
        crate::x86::types::Mnem::Lods => {
            let loaded = load_at(abi::ESI);
            let mut v;
            if size == 4 {
                v = set_reg(abi::EAX, loaded);
                v.extend(bump(abi::ESI, 1));
            } else {
                let mut eax = reg(abi::EAX);
                eax.extend(i32c(if size == 1 { !0xff } else { !0xffff }));
                eax.push(Op::I32_AND);
                eax.extend(loaded);
                eax.push(Op::I32_OR);
                v = set_reg(abi::EAX, eax);
                v.extend(bump(abi::ESI, 1));
            }
            v
        }
        crate::x86::types::Mnem::Scas => {
            let mut v = cmp_vals(
                {
                    let mut x = reg(abi::EAX);
                    x.extend(size_mask(size as u32));
                    x
                },
                load_at(abi::EDI),
            );
            v.extend(bump(abi::EDI, 1));
            v
        }
        crate::x86::types::Mnem::Cmps => {
            let mut v = cmp_vals(load_at(abi::ESI), load_at(abi::EDI));
            v.extend(bump(abi::ESI, 1));
            v.extend(bump(abi::EDI, 1));
            v
        }
        _ => panic!("bad string op"),
    };

    let has_rep = !matches!(rep, crate::x86::types::Rep::None);
    if !has_rep {
        return body;
    }

    // repe/repne termination on scas/cmps: exit when ZF no longer matches
    let zf = {
        let mut v = reg(abi::FLAGS);
        v.extend(i32c(0x40));
        v.push(Op::I32_AND);
        v
    };
    let mismatch = if rep == crate::x86::types::Rep::WhileNotEqual {
        zf
    } else {
        let mut v = zf;
        v.push(Op::I32_EQZ);
        v
    };
    let check_zf: Seq = if insn.mnem == crate::x86::types::Mnem::Scas
        || insn.mnem == crate::x86::types::Mnem::Cmps
    {
        let mut v = mismatch;
        v.push(Op::BR_IF);
        v.push(1);
        v
    } else {
        Vec::new()
    };

    let mut out: Seq = vec![Op::BLOCK, 0x40, Op::LOOP, 0x40];
    out.extend(reg(abi::ECX));
    out.push(Op::I32_EQZ);
    out.push(Op::BR_IF);
    out.push(1);
    if checkpoint_import.is_some() {
        // ECX is guest architectural state, so no private counter survives a yield.
        out.extend(reg(abi::ECX));
        out.extend(i32c(crate::translate::scheduling::REP_INTERVAL - 1));
        out.push(Op::I32_AND);
        out.push(Op::I32_EQZ);
        out.extend([Op::IF, 0x40]);
        out.extend(crate::translate::scheduling::checkpoint(
            checkpoint_import,
            insn.va,
        ));
        out.push(Op::END);
    }
    if insn.mnem == crate::x86::types::Mnem::Stos {
        out.extend(crate::translate::kernels::repeated_store(size));
    }
    out.extend(body);
    out.extend(set_reg(abi::ECX, {
        let mut v = reg(abi::ECX);
        v.extend(i32c(1));
        v.push(Op::I32_SUB);
        v
    }));
    out.extend(check_zf);
    out.push(Op::BR);
    out.push(0);
    out.push(Op::END);
    out.push(Op::END);
    out
}
