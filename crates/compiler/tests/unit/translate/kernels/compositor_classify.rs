use super::*;
use crate::{
    guestmem::GuestMem,
    translate::{
        emit::{emit_insn, EmitCtx},
        layout::NEXT,
        scheduling::CHECKPOINT_BUDGET_GLOBAL,
        test_support::execute_sequences_v8,
    },
    x86::decode::decode_insn,
};
use std::collections::{BTreeMap, HashMap};

#[test]
fn alpha_classification_preserves_masks_aliases_faults_and_all_checkpoint_paths() {
    let functions = HashMap::new();
    let imports = HashMap::new();
    let context = EmitCtx {
        memory_minimum_pages: 1,
        id: BTreeMap::new(),
        func_idx_of: &functions,
        idx_of: 0,
        iat_shim: &imports,
        block_trace: None,
    };
    let native = |hex: &str| {
        let bytes: Vec<_> = (0..hex.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
            .collect();
        let mut image = GuestMem::new();
        image.write(0, &bytes);
        let mut code = Vec::new();
        let mut at = 0;
        while at < bytes.len() as u32 {
            let insn = decode_insn(&image, 0, at).unwrap();
            at += insn.size;
            code.extend(emit_insn(&insn, &context, -1, 0).unwrap());
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
    let high_test = || {
        let mut code = poll(0x4988e1);
        code.extend(native("3df7000000"));
        code.extend(binary(reg(abi::EAX), i32c(247), Op::I32_GT_S));
        code.extend([Op::IF, 0x40]);
        code.extend(poll(0x4988e8));
        code.extend(native("0975f0"));
        code.extend(i32c(1));
        code.extend(local_set(INDEX));
        code.push(Op::END);
        code
    };
    let mut routines = Vec::new();
    for (name, optimized) in [("plain", false), ("span", true)] {
        let mut code = i32c(0);
        code.extend(local_set(NEXT));
        code.extend([Op::BLOCK, 0x40, Op::LOOP, 0x40]);
        code.extend(local_get(NEXT));
        code.extend([Op::BR_IF, 1]);
        code.extend(i32c(0));
        code.extend(local_set(INDEX));
        code.extend(poll(0x4988c6));
        if optimized {
            code.extend(emit(1, 0, true));
        }
        code.extend(native("8b45100fb6040103452083f807"));
        code.extend(binary(reg(abi::EAX), i32c(7), Op::I32_GT_S));
        code.push(Op::I32_EQZ);
        code.extend([Op::IF, 0x40]);
        code.extend(high_test());
        code.push(Op::ELSE);
        code.extend(poll(0x4988d5));
        code.extend(native("3df8000000"));
        code.extend(binary(reg(abi::EAX), i32c(248), Op::I32_LT_S));
        code.push(Op::I32_EQZ);
        code.extend([Op::IF, 0x40]);
        code.extend(high_test());
        code.push(Op::ELSE);
        code.extend(poll(0x4988dc));
        code.extend(native("09751c"));
        code.extend([Op::END, Op::END]);
        code.extend(local_get(INDEX));
        code.extend([Op::I32_EQZ, Op::IF, 0x40]);
        code.extend(poll(0x4988eb));
        code.push(Op::END);
        code.extend(native("668902414242d1e681fa4c5c6e00"));
        code.extend(binary(reg(abi::EDX), i32c(ALPHA + 64), Op::I32_LT_S));
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
        include_str!("../../../fixtures/compositor-alpha-classify.mjs"),
    );
}

#[test]
fn classification_branch_paths_match_discovered_native_blocks() {
    let text="8b45100fb6040103452083f8077e0c3df80000007d0509751ceb0a3df70000007e030975f0668902414242d1e681fa4c5c6e007ccbc3";
    let bytes: Vec<_> = (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap())
        .collect();
    let mut memory = GuestMem::new();
    memory.write(0, &bytes);
    let range = (0x4988c6, 0x4988c6 + bytes.len() as u32);
    let (blocks, index) = crate::translate::blocks::split_blocks(
        &memory,
        range.0,
        range.0,
        Some(range),
        &[range],
        &mut Vec::new(),
    )
    .unwrap();
    let mut starts: Vec<_> = index.keys().copied().collect();
    starts.sort_unstable();
    assert_eq!(
        starts,
        vec![0x4988c6, 0x4988d5, 0x4988dc, 0x4988e1, 0x4988e8, 0x4988eb, 0x4988fb]
    );
    assert_eq!(blocks[index[&0x4988eb]].insns.last().unwrap().va, 0x4988f9);
    // The opaque block overlaps the shared update; it does not poll again at EB.
    for (start, end) in [
        (0x4988c6, 0x4988d3),
        (0x4988d5, 0x4988da),
        (0x4988dc, 0x4988df),
        (0x4988e1, 0x4988e6),
        (0x4988e8, 0x4988f9),
    ] {
        assert_eq!(blocks[index[&start]].insns.last().unwrap().va, end);
    }
    assert!(blocks[index[&0x4988e8]]
        .insns
        .iter()
        .any(|i| i.va == 0x4988eb));
}
