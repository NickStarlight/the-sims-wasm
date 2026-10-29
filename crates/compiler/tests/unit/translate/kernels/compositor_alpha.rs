use super::*;
use crate::{
    guestmem::GuestMem,
    translate::{emit::emit_insn, test_support::execute_sequences_v8},
    x86::decode::decode_insn,
};
use std::collections::{BTreeMap, HashMap};

// Exact straight-line loop body from the fingerprinted routine, before JL.
const LOOP: &str = "8b4d1c0fb6860c5c6e000fb7090fb7960c5d6e008bf98bdac1ef05c1e803d1efc1e00583e71f8945f4c1eb0503f8d1ebc1e70583e31f660fb6bc1f5c5e6e008bd983e11fc1eb0b83e31f034df403d88bc2c1e80bc1e30583e01f83e21fc1e105660fb684035c5e6e00c1e0050bf8660fb684115c5e6e008b4d1c83451c02c1e7060bf88b4508464666893c0883fe40";

#[test]
fn alpha_spans_preserve_checkpoint_register_memory_and_fault_semantics() {
    use crate::translate::{layout::NEXT, scheduling::CHECKPOINT_BUDGET_GLOBAL};
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = crate::translate::emit::EmitCtx {
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    let bytes: Vec<_> = (0..LOOP.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&LOOP[i..i + 2], 16).unwrap())
        .collect();
    let mut memory = GuestMem::new();
    memory.write(0, &bytes);
    let mut decoded = Vec::new();
    let mut at = 0;
    while at < bytes.len() as u32 {
        let mut instruction = decode_insn(&memory, 0, at).unwrap();
        at += instruction.size;
        instruction.va += 0x49892f;
        decoded.push(instruction);
    }
    let mut routines = Vec::new();
    for (name, optimized) in [("plain", false), ("span", true)] {
        let mut code = i32c(0);
        code.extend(local_set(NEXT));
        code.extend([Op::BLOCK, 0x40, Op::LOOP, 0x40]);
        code.extend(local_get(NEXT));
        code.extend([Op::BR_IF, 1]);
        code.extend(set_reg(abi::TRAP_VA, i32c(0x49892f)));
        code.extend(set_reg(
            CHECKPOINT_BUDGET_GLOBAL,
            binary(reg(CHECKPOINT_BUDGET_GLOBAL), i32c(1), Op::I32_SUB),
        ));
        code.extend(reg(CHECKPOINT_BUDGET_GLOBAL));
        code.extend([Op::I32_EQZ, Op::IF, 0x40]);
        code.extend(set_reg(CHECKPOINT_BUDGET_GLOBAL, i32c(256)));
        code.extend([Op::CALL, 0, Op::END]);
        if optimized {
            code.extend(full_span(1, 0, true));
        }
        for instruction in &decoded {
            code.extend(emit_insn(instruction, &context, -1, 0).unwrap());
        }
        code.extend(binary(reg(abi::ESI), i32c(64), Op::I32_LT_S));
        code.extend([Op::BR_IF, 0, Op::END, Op::END]);
        routines.push((name, crate::translate::optimize::promote(&code).0));
    }
    routines.push((
        "set_budget",
        set_reg(CHECKPOINT_BUDGET_GLOBAL, reg(abi::EAX)),
    ));
    routines.push((
        "read_budget",
        set_reg(abi::EAX, reg(CHECKPOINT_BUDGET_GLOBAL)),
    ));
    execute_sequences_v8(
        &routines,
        include_str!("../../../fixtures/compositor-alpha-span.mjs"),
    );
}

const MASK_TEST: &str = "8b451c8545f4";
const MASK_BLEND: &str = "8b4df80fbf860c5c6e000fb7090fb7960c5d6e008bf98bdac1ef05c1e803d1efc1e00583e71f8945d8c1eb0503f8d1ebc1e70583e31f660fb6bc1f5c5e6e008bd983e11fc1eb0b83e31f034dd803d88bc2c1e80bc1e30583e01f83e21fc1e105660fb684035c5e6e00c1e0050bf8660fb684115c5e6e008b4df8c1e7060bf88b450866893c08";
const MASK_STEP: &str = "8345f80246d165f44683fe40";

#[test]
fn masked_alpha_spans_preserve_every_native_checkpoint_path() {
    use crate::translate::{layout::NEXT, scheduling::CHECKPOINT_BUDGET_GLOBAL};
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = crate::translate::emit::EmitCtx {
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    let emit_bytes = |text: &str| {
        let bytes: Vec<_> = (0..text.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
            .collect();
        let mut memory = GuestMem::new();
        memory.write(0, &bytes);
        let mut code = Vec::new();
        let mut at = 0;
        while at < bytes.len() as u32 {
            let instruction = decode_insn(&memory, 0, at).unwrap();
            at += instruction.size;
            code.extend(emit_insn(&instruction, &context, -1, 0).unwrap());
        }
        code
    };
    let poll = |address| {
        let mut code = set_reg(abi::TRAP_VA, i32c(address));
        code.extend(set_reg(
            CHECKPOINT_BUDGET_GLOBAL,
            binary(reg(CHECKPOINT_BUDGET_GLOBAL), i32c(1), Op::I32_SUB),
        ));
        code.extend(reg(CHECKPOINT_BUDGET_GLOBAL));
        code.extend([Op::I32_EQZ, Op::IF, 0x40]);
        code.extend(set_reg(CHECKPOINT_BUDGET_GLOBAL, i32c(256)));
        code.extend([Op::CALL, 0, Op::END]);
        code
    };
    let mut routines = Vec::new();
    for (name, optimized) in [("plain", false), ("span", true)] {
        let mut code = i32c(0);
        code.extend(local_set(NEXT));
        code.extend([Op::BLOCK, 0x40, Op::LOOP, 0x40]);
        code.extend(local_get(NEXT));
        code.extend([Op::BR_IF, 1]);
        code.extend(poll(0x4989ff));
        if optimized {
            code.extend(masked_span(1, 0, true));
        }
        code.extend(emit_bytes(MASK_TEST));
        code.extend(binary(reg(abi::FLAGS), i32c(abi::ZF), Op::I32_AND));
        code.extend([Op::I32_EQZ, Op::IF, 0x40]);
        code.extend(poll(0x498a0b));
        code.extend(emit_bytes(MASK_BLEND));
        code.push(Op::ELSE);
        code.extend(poll(0x498a91));
        code.push(Op::END);
        code.extend(emit_bytes(MASK_STEP));
        code.extend(binary(reg(abi::ESI), i32c(64), Op::I32_LT_S));
        code.extend([Op::BR_IF, 0, Op::END, Op::END]);
        routines.push((name, crate::translate::optimize::promote(&code).0));
    }
    routines.push((
        "set_budget",
        set_reg(CHECKPOINT_BUDGET_GLOBAL, reg(abi::EAX)),
    ));
    routines.push((
        "read_budget",
        set_reg(abi::EAX, reg(CHECKPOINT_BUDGET_GLOBAL)),
    ));
    execute_sequences_v8(
        &routines,
        include_str!("../../../fixtures/compositor-alpha-masked.mjs"),
    );
}

// Pin the real segment construction too: the blend block includes its trailing
// increment instructions, while the conditional skip enters those same bytes.
#[test]
fn masked_span_budget_matches_discovered_overlapping_blocks() {
    let text="8b451c8545f40f84860000008b4df80fbf860c5c6e000fb7090fb7960c5d6e008bf98bdac1ef05c1e803d1efc1e00583e71f8945d8c1eb0503f8d1ebc1e70583e31f660fb6bc1f5c5e6e008bd983e11fc1eb0b83e31f034dd803d88bc2c1e80bc1e30583e01f83e21fc1e105660fb684035c5e6e00c1e0050bf8660fb684115c5e6e008b4df8c1e7060bf88b450866893c088345f80246d165f44683fe400f8c5cffffffc3";
    let bytes: Vec<_> = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect();
    let mut memory = GuestMem::new();
    memory.write(0, &bytes);
    let range = (0x4989ff, 0x4989ff + bytes.len() as u32);
    let (blocks, index) = crate::translate::blocks::split_blocks(
        &memory,
        range.0,
        range.0,
        Some(range),
        &[range],
        &mut Vec::new(),
    )
    .unwrap();
    assert_eq!(index.len(), 4);
    for (start, end) in [
        (0x4989ff, 0x498a05),
        (0x498a0b, 0x498a9d),
        (0x498a91, 0x498a9d),
    ] {
        let block = &blocks[index[&start]];
        assert_eq!(block.insns.last().unwrap().va, end);
    }
    assert!(blocks[index[&0x498a0b]]
        .insns
        .iter()
        .any(|insn| insn.va == 0x498a91));
}
