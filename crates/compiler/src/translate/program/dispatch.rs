//! Guest address lookup, entry points, and host-call trampolines.

use crate::translate::abi;
use crate::translate::emit_helpers::{emit_store, i32c, local_get, local_set, reg, set_reg, Seq};
use crate::translate::layout::locals_list;
use crate::wasm::{
    builder::{FuncDef, WasmBuilder},
    leb::sleb,
    opcodes::{Op, I32},
};

/// Emit `$idxOf(addr) → table index`: API identity range + binary search.
pub(super) fn emit_idx_of(
    b: &mut WasmBuilder,
    _self_idx: i32,
    func_count: u32,
    table_addr: u32,
) -> u32 {
    let t = b.add_type(&[I32], &[I32]);
    let locals: Vec<u8> = vec![I32, I32, I32]; // lo, hi, mid
    let mut body: Vec<u8> = Vec::new();
    let p = |body: &mut Vec<u8>| {
        body.push(Op::LOCAL_GET);
        body.push(0);
    };

    // API identity range → identity slot (a RANGE check, not a floor)
    p(&mut body);
    body.extend(i32c(abi::API_BASE as i32));
    body.push(Op::I32_GE_U);
    p(&mut body);
    body.extend(i32c(abi::API_END as i32));
    body.push(Op::I32_LT_U);
    body.push(Op::I32_AND);
    body.push(Op::IF);
    body.push(0x40);
    p(&mut body);
    body.extend(i32c(abi::API_BASE as i32));
    body.push(Op::I32_SUB);
    body.extend(i32c(2));
    body.push(Op::I32_SHR_U);
    body.extend(i32c(func_count as i32 + 1));
    body.push(Op::I32_ADD);
    body.push(Op::RETURN);
    body.push(Op::END);

    // binary search: largest A[i] <= addr
    body.extend(i32c(0));
    body.extend(local_set(1)); // lo
    body.extend(i32c(func_count as i32));
    body.extend(local_set(2)); // hi
    body.push(Op::BLOCK);
    body.push(0x40);
    body.push(Op::LOOP);
    body.push(0x40);
    body.extend(local_get(1));
    body.extend(local_get(2));
    body.push(Op::I32_LT_U);
    body.push(Op::I32_EQZ);
    body.push(Op::BR_IF);
    body.push(1);
    body.extend(local_get(1));
    body.extend(local_get(2));
    body.push(Op::I32_ADD);
    body.extend(i32c(1));
    body.push(Op::I32_SHR_U);
    body.extend(local_set(3));
    body.extend(i32c(table_addr as i32));
    body.extend(local_get(3));
    body.extend(i32c(4));
    body.push(Op::I32_MUL);
    body.push(Op::I32_ADD);
    body.push(Op::I32_LOAD);
    body.push(2);
    body.push(0);
    p(&mut body);
    body.push(Op::I32_LE_U);
    body.push(Op::IF);
    body.push(0x40);
    body.extend(local_get(3));
    body.extend(i32c(1));
    body.push(Op::I32_ADD);
    body.extend(local_set(1));
    body.push(Op::ELSE);
    body.extend(local_get(3));
    body.extend(local_set(2));
    body.push(Op::END);
    body.push(Op::BR);
    body.push(0);
    body.push(Op::END);
    body.push(Op::END);
    body.extend(i32c(table_addr as i32));
    body.extend(local_get(1));
    body.extend(i32c(1));
    body.push(Op::I32_SUB);
    body.extend(i32c(4));
    body.push(Op::I32_MUL);
    body.push(Op::I32_ADD);
    body.push(Op::I32_LOAD);
    body.push(2);
    body.push(0);
    p(&mut body);
    body.push(Op::I32_EQ);
    body.push(Op::IF);
    body.push(0x40);
    body.extend(local_get(1));
    body.push(Op::RETURN);
    body.push(Op::END);
    body.extend(i32c(0)); // unknown

    b.add_func(FuncDef {
        type_idx: t,
        locals,
        body,
    })
}

/// Exported `run(addr)`: dispatch into the table.
pub(super) fn emit_run(b: &mut WasmBuilder, void_type: u32, idx_of: i32) -> u32 {
    const ESP: u8 = 4; // R.ESP
    let push_stack = |value: Seq| -> Seq {
        let mut esp_upd = reg(ESP);
        esp_upd.extend(i32c(-4));
        esp_upd.push(Op::I32_ADD);
        let mut v = set_reg(ESP, esp_upd);
        v.extend(emit_store(
            &crate::x86::types::MemOp {
                base: ESP as i8,
                index: -1,
                scale: 1,
                disp: 0,
                size: 4,
                fs: false,
            },
            value,
        ));
        v
    };
    let mut body: Seq = set_reg(abi::RESUME_NEXT, i32c(0));
    body.extend(push_stack(i32c(10))); // nCmdShow = SW_SHOW
    body.extend(push_stack(i32c(abi::COMMAND_LINE_ADDR as i32))); // lpCmdLine → host string
    body.extend(push_stack(i32c(0))); // hPrevInstance
    body.extend(push_stack(i32c(0x400000))); // hInstance
    body.extend(push_stack(i32c(0))); // fake return address
    body.push(Op::LOCAL_GET);
    body.push(0);
    body.push(Op::CALL);
    body.extend(sleb(idx_of));
    body.push(Op::CALL_INDIRECT);
    body.push(0);
    body.push(0);
    b.add_func(FuncDef {
        type_idx: void_type,
        locals: locals_list(),
        body,
    })
}

/// Exported `runAt(funcVa)`: dispatch into the table like `run`, no WinMain args.
pub(super) fn emit_run_at(b: &mut WasmBuilder, void_type: u32, idx_of: i32) -> u32 {
    let mut body: Seq = Vec::new();
    body.push(Op::LOCAL_GET);
    body.push(0);
    body.push(Op::CALL);
    body.extend(sleb(idx_of));
    body.push(Op::CALL_INDIRECT);
    body.push(0);
    body.push(0);
    b.add_func(FuncDef {
        type_idx: void_type,
        locals: vec![],
        body,
    })
}

pub(super) fn tramp_body(k: u32, dispatch_import: i32) -> Vec<u8> {
    let mut v = i32c(k as i32);
    v.push(Op::CALL);
    v.extend(sleb(dispatch_import));
    v
}
