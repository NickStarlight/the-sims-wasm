//! Emit one guest function and its instruction diagnostics.

use super::flow::ForwardFlow;
use super::model::InsnMapEntry;
use super::segments::{build_segments, seg_ids};
use crate::translate::emit_helpers::{i32c, reg, set_reg, Seq};
use crate::translate::error::EmitError;
use crate::translate::layout::{ENTRY_SP, NEXT};
use crate::translate::scheduling::{block_checkpoint, checkpoint, STRAIGHT_LINE_INTERVAL};
use crate::translate::{
    abi,
    blocks::Block,
    emit::{emit_insn, EmitCtx},
};
use crate::wasm::{
    leb::{sleb, uleb},
    opcodes::Op,
};
use std::collections::HashMap;

/// Dispatch resumable segments with a jump table inside the checkpointed loop.
pub(super) fn emit_function(
    blocks: &[Block],
    block_index: &HashMap<u32, usize>,
    ctx: &EmitCtx,
    entry_va: u32,
    map: &mut Vec<InsnMapEntry>,
    diagnostics: &mut Vec<String>,
    kernel: Option<&crate::translate::kernels::Kernel>,
) -> Result<Seq, EmitError> {
    let (id_map, _) = seg_ids(blocks, block_index);
    let segs = build_segments(blocks, block_index);
    let ordered: Vec<u32> = segs.keys().copied().collect();
    let mut out: Seq = Vec::new();

    out.extend(reg(abi::ESP));
    out.extend(crate::translate::emit_helpers::local_set(ENTRY_SP));
    out.extend(reg(abi::RESUME_NEXT)); // resume entry: block id from the one-shot global
    out.extend(crate::translate::emit_helpers::local_set(NEXT)); // …then clear it
    out.extend(set_reg(abi::RESUME_NEXT, i32c(0)));
    let entry_id = id_map.get(&entry_va).copied().unwrap_or(0);
    // NEXT is a wasm local, not guest EAX (global 0). Testing EAX here skips
    // high-address wrappers that tail-share a lower-address function body.
    out.push(Op::LOCAL_GET);
    out.extend(sleb(NEXT as i32));
    out.push(Op::I32_EQZ);
    out.push(Op::IF);
    out.push(0x40);
    out.extend(i32c(entry_id));
    out.extend(crate::translate::emit_helpers::local_set(NEXT));
    out.push(Op::END);
    out.push(Op::BLOCK);
    out.push(0x40); // $exit
    out.push(Op::LOOP);
    out.push(0x40); // $top

    // Nest one label per segment. Ascending bodies leave forward destinations
    // in enclosing labels; NEXT still uses the original stable segment IDs.
    for _ in &ordered {
        out.extend([Op::BLOCK, 0x40]);
    }
    out.extend(crate::translate::emit_helpers::local_get(NEXT));
    out.push(Op::BR_TABLE);
    out.extend(uleb(ordered.len() as u32));
    for depth in 0..ordered.len() {
        out.extend(uleb(depth as u32));
    }
    out.extend(uleb(ordered.len() as u32 + 1)); // invalid NEXT exits the function

    for (i, va) in ordered.iter().enumerate() {
        out.push(Op::END); // close this segment's entry label
        let flow = ForwardFlow::new(i, ordered.len());
        let loop_depth = flow.loop_depth() as i32;
        let seg = &segs[va];
        out.extend(block_checkpoint(ctx.block_trace, *va));
        if let Some(kernel) = kernel {
            if let Some(kernel) = kernel.emit(*va, ctx, loop_depth) {
                let first = &blocks[seg.insn_indices[0].0].insns[seg.insn_indices[0].1];
                map.push(InsnMapEntry {
                    wasm_off: out.len() as u32,
                    guest_va: first.va,
                    mnem: first.mnem,
                    flags: first.flags,
                    rel: first.rel.map(|r| r as i32),
                });
                out.extend_from_slice(&kernel);
            }
        }
        let succ_id = seg
            .succ_va
            .and_then(|sv| id_map.get(&sv).copied())
            .unwrap_or(-1);
        if succ_id == -1 && !seg.ends_flow {
            diagnostics.push(format!("unresolved successor in block {va:#x}"));
        }
        let mut position = 0;
        while position < seg.insn_indices.len() {
            let (bi, ii) = seg.insn_indices[position];
            let insn = &blocks[bi].insns[ii];
            if position > 0 && position % STRAIGHT_LINE_INTERVAL == 0 {
                out.extend(checkpoint(ctx.block_trace, insn.va));
            }
            if let Some(kernel) = kernel.filter(|kernel| kernel.sequence_end(insn.va).is_some()) {
                let end = ((position / STRAIGHT_LINE_INTERVAL + 1) * STRAIGHT_LINE_INTERVAL)
                    .min(seg.insn_indices.len());
                let candidates: Vec<_> = seg.insn_indices[position..end]
                    .iter()
                    .map(|&(bi, ii)| &blocks[bi].insns[ii])
                    .collect();
                if let Some(run) = kernel.sequence(&candidates, ctx) {
                    for (index, offset) in run.offsets {
                        let insn = candidates[index];
                        map.push(InsnMapEntry {
                            wasm_off: out.len() as u32 + offset,
                            guest_va: insn.va,
                            mnem: insn.mnem,
                            flags: insn.flags,
                            rel: insn.rel.map(|r| r as i32),
                        });
                    }
                    out.extend(run.code);
                    position += run.count;
                    continue;
                }
            }
            if insn.mnem == crate::x86::types::Mnem::Fpu {
                let end = ((position / STRAIGHT_LINE_INTERVAL + 1) * STRAIGHT_LINE_INTERVAL)
                    .min(seg.insn_indices.len());
                let candidates: Vec<_> = seg.insn_indices[position..end]
                    .iter()
                    .map(|&(bi, ii)| &blocks[bi].insns[ii])
                    .collect();
                let run = crate::translate::emit_fpu::integer_copy::emit(&candidates)
                    .or_else(|| crate::translate::emit_fpu::runs::emit(&candidates));
                if let Some(run) = run {
                    for (index, offset) in run.offsets {
                        let insn = candidates[index];
                        map.push(InsnMapEntry {
                            wasm_off: out.len() as u32 + offset,
                            guest_va: insn.va,
                            mnem: insn.mnem,
                            flags: insn.flags,
                            rel: insn.rel.map(|r| r as i32),
                        });
                    }
                    out.extend(run.code);
                    position += run.count;
                    continue;
                }
            }
            map.push(InsnMapEntry {
                wasm_off: out.len() as u32,
                guest_va: insn.va,
                mnem: insn.mnem,
                flags: insn.flags,
                rel: insn.rel.map(|r| r as i32),
            });
            let optimized = flow.instruction(insn, ctx, succ_id).or_else(|| {
                seg.insn_indices
                    .get(position + 1)
                    .filter(|_| (position + 1) % STRAIGHT_LINE_INTERVAL != 0)
                    .and_then(|&(next_block, next_insn)| {
                        crate::translate::emit::emit_overwritten_flags(
                            insn,
                            &blocks[next_block].insns[next_insn],
                        )
                    })
            });
            let piece = match optimized {
                Some(piece) => piece,
                None => emit_insn(insn, ctx, succ_id, loop_depth)?,
            };
            out.extend(piece);
            position += 1;
        }
        let &(bi, ii) = seg.insn_indices.last().unwrap();
        if !seg.ends_flow
            && !matches!(
                blocks[bi].insns[ii].mnem,
                crate::x86::types::Mnem::Jcc | crate::x86::types::Mnem::Jcxz
            )
        {
            out.extend(flow.transfer(succ_id));
        }
    }

    out.push(Op::BR); // defensive fallthrough to $exit
    out.push(1);
    out.push(Op::END); // loop
    out.push(Op::END); // $exit
    let (out, offsets) = crate::translate::optimize::promote(&out);
    for instruction in map {
        let index = offsets
            .binary_search_by_key(&instruction.wasm_off, |&(before, _)| before)
            .expect("instruction map must point at an operator boundary");
        instruction.wasm_off = offsets[index].1;
    }
    Ok(out)
}
