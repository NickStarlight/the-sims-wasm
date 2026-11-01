//! Direct and indirect guest calls and return-address setup.

use super::{
    abi, emit_load, emit_store, i32c, local_get, local_set, mem_at, read_op, reg, set_reg, sleb,
    EmitCtx, Op, Operand, Seq, ESP, T3,
};

/// Push a fake return address, then call.
pub(super) fn push_fake_ret(fake_ret: u32) -> Seq {
    let mut esp_upd = reg(ESP);
    esp_upd.extend(i32c(-4));
    esp_upd.push(Op::I32_ADD);
    let mut v = set_reg(ESP, esp_upd);
    v.extend(emit_store(&mem_at(ESP, 0, 4), i32c(fake_ret as i32)));
    v
}

pub(super) fn call_direct(fn_idx: i32, fake_ret: Option<u32>) -> Seq {
    let mut out = Vec::new();
    if let Some(fr) = fake_ret {
        out.extend(push_fake_ret(fr));
    }
    out.push(Op::CALL);
    out.extend(sleb(fn_idx));
    out
}

pub(super) fn call_indirect(target: &Operand, ctx: &EmitCtx, fake_ret: Option<u32>) -> Seq {
    let pre: Seq = match fake_ret {
        Some(fr) => push_fake_ret(fr),
        None => Vec::new(),
    };
    // absolute address = static IAT slot → resolve to its shim's table index
    if let Operand::Mem(m) = target {
        if m.base == -1 && m.index == -1 {
            let idx = ctx.iat_shim.get(&(m.disp as u32)).copied().unwrap_or(-1);
            if idx >= 0 {
                let mut v = pre;
                v.push(Op::I32_CONST);
                v.extend(sleb(idx));
                v.push(Op::CALL_INDIRECT);
                v.push(0);
                v.push(0);
                return v;
            }
        }
    }
    // otherwise compute the target VALUE
    let target_val = match target {
        Operand::Mem(m) => emit_load(m),
        other => read_op(other),
    };
    // CALL reads its operand before pushing the return address. In particular,
    // [ESP+offset] and ESP itself refer to the caller's original stack pointer.
    let mut v = target_val;
    v.extend(local_set(T3));
    v.extend(pre);
    v.extend(local_get(T3));
    v.push(Op::GLOBAL_SET);
    v.push(abi::CALL_TARGET);
    v.extend(local_get(T3));
    v.push(Op::CALL);
    v.extend(sleb(ctx.idx_of));
    v.push(Op::CALL_INDIRECT);
    v.push(0);
    v.push(0);
    v
}
