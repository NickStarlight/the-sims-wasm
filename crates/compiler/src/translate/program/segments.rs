//! Split basic blocks into resumable call-boundary segments.

use crate::translate::blocks::Block;
use crate::x86::types::Mnem;
use std::collections::{BTreeMap, HashMap};

pub(super) struct Seg {
    pub(super) insn_indices: Vec<(usize, usize)>, // (block index, insn index) into the blocks vec
    pub(super) succ_va: Option<u32>,
    pub(super) ends_flow: bool,
}

/// Dispatch segments: basic blocks further split at every `call` instruction.
pub(super) fn build_segments(
    blocks: &[Block],
    block_index: &HashMap<u32, usize>,
) -> BTreeMap<u32, Seg> {
    let mut segs: BTreeMap<u32, Seg> = BTreeMap::new();
    for (bi, block) in blocks.iter().enumerate() {
        let mut start = block.va;
        let mut cur: Vec<(usize, usize)> = Vec::new();
        let n = block.insns.len();
        for (i, insn) in block.insns.iter().enumerate() {
            cur.push((bi, i));
            if insn.mnem == Mnem::Call && i < n - 1 {
                segs.insert(
                    start,
                    Seg {
                        insn_indices: std::mem::take(&mut cur),
                        succ_va: Some(insn.va + insn.size),
                        ends_flow: false,
                    },
                );
                start = insn.va + insn.size;
            }
        }
        let last = block.insns.last().unwrap();
        let ends_flow = is_terminator(block);
        let succ_va = if ends_flow {
            None
        } else if last.mnem == Mnem::Jcc || last.mnem == Mnem::Jcxz {
            block.succs.get(1).copied()
        } else {
            block.succs.first().copied()
        };
        let _ = block_index;
        segs.insert(
            start,
            Seg {
                insn_indices: cur,
                succ_va,
                ends_flow,
            },
        );
    }
    segs
}

pub(super) fn seg_ids(
    blocks: &[Block],
    block_index: &HashMap<u32, usize>,
) -> (BTreeMap<u32, i32>, Vec<u32>) {
    let segs = build_segments(blocks, block_index);
    let keys: Vec<u32> = segs.keys().copied().collect();
    let id: BTreeMap<u32, i32> = keys
        .iter()
        .enumerate()
        .map(|(i, va)| (*va, i as i32))
        .collect();
    (id, keys)
}

fn is_terminator(block: &Block) -> bool {
    let last = block.insns.last().unwrap();
    last.mnem == Mnem::Ret
        || last.mnem == Mnem::Int3
        || last.mnem == Mnem::Ud2
        || last.mnem == Mnem::Jmp
}
