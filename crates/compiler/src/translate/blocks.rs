//! Split a decoded instruction stream into basic blocks keyed by start VA.
//! A deterministic LIFO worklist preserves block discovery order. Speculative
//! jump-table branches with invalid instructions are discarded.

use crate::guestmem::GuestMem;
use crate::x86::decode::decode_insn;
use crate::x86::types::{Insn, Mnem};
use std::collections::HashMap;

const MAX_FUNCTION_INSTRUCTIONS: usize = 100_000;

pub struct Block {
    pub va: u32,
    pub insns: Vec<Insn>,
    /// Branch targets this block can flow to (empty for ret/indirect-jmp enders).
    pub succs: Vec<u32>,
}

/// Static case targets of a switch jump-table jmp.
fn switch_targets(image: &GuestMem, base: u32, insn: &Insn, text: (u32, u32)) -> Vec<u32> {
    let op = match insn.ops.first() {
        Some(crate::x86::types::Operand::Mem(m)) => m,
        _ => return vec![],
    };
    let m = op;
    if m.index == -1 || m.scale != 4 || m.base != -1 {
        return vec![];
    }
    let mut out: Vec<u32> = Vec::new();
    let mut invalid_run = 0;
    let mut table = m.disp as i64;
    while out.len() < 256 && invalid_run < 2 {
        let p = table - base as i64;
        if p < 0 || p + 4 > crate::guestmem::GUEST_SIZE as i64 {
            break;
        }
        let v = image.u32_le(p as usize);
        if v >= text.0 && v < text.1 {
            out.push(v);
            invalid_run = 0;
        } else {
            invalid_run += 1;
        }
        table += 4;
    }
    // Optimized CRT loops also use negative indices, with the displacement at
    // the end (or one past the end) of a table. Read only the contiguous pointer
    // run before it; crossing a non-pointer would speculate into unrelated code.
    table = m.disp as i64 - 4;
    for _ in 0..256 {
        let p = table - base as i64;
        if p < 0 || p + 4 > crate::guestmem::GUEST_SIZE as i64 {
            break;
        }
        let v = image.u32_le(p as usize);
        if v < text.0 || v >= text.1 {
            break;
        }
        out.push(v);
        table -= 4;
    }
    out
}

/// Linear sweep from `entry`, splitting at branch targets; stops at ret or decoded gap.
/// Returns blocks in discovery order, bounded by executable ranges and a decode budget.
pub fn split_blocks(
    image: &GuestMem,
    base: u32,
    entry: u32,
    text: Option<(u32, u32)>,
    executable: &[(u32, u32)],
    diagnostics: &mut Vec<String>,
) -> Result<(Vec<Block>, HashMap<u32, usize>), String> {
    let mut decoded_instructions = 0;
    let mut blocks: Vec<Block> = Vec::new();
    let mut index: HashMap<u32, usize> = HashMap::new();
    type QEntry = (u32, bool); // (va, spec)
    let mut queue: Vec<QEntry> = vec![(entry, false)];
    while let Some((start, spec)) = queue.pop() {
        if blocks.len() > 50000 {
            return Err("block explosion (bogus decode region)".into());
        }
        if index.contains_key(&start) {
            continue;
        }
        let mut insns: Vec<Insn> = Vec::new();
        let mut succs: Vec<u32> = Vec::new();
        let mut va = start;
        let mut poisoned = false;
        loop {
            let Some(&(_, section_end)) = executable.iter().find(|&&(lo, hi)| va >= lo && va < hi)
            else {
                if spec {
                    poisoned = true;
                    break;
                }
                return Err(format!(
                    "instruction {va:#x} is outside mapped executable sections"
                ));
            };
            if decoded_instructions >= MAX_FUNCTION_INSTRUCTIONS {
                return Err(format!(
                    "function {entry:#x} exceeds instruction budget ({MAX_FUNCTION_INSTRUCTIONS})"
                ));
            }
            decoded_instructions += 1;
            let insn = match decode_insn(image, base, va) {
                Ok(i) => i,
                Err(error) => {
                    if spec {
                        poisoned = true; // bad table over-read: not a real case, drop it
                        break;
                    }
                    // undecodable byte: mark the gap (ud2 = never a real instruction here)
                    insns.push(Insn {
                        va,
                        size: 1,
                        mnem: Mnem::Ud2,
                        ops: vec![],
                        cc: None,
                        rel: None,
                        rep: crate::x86::types::Rep::None,
                        flags: crate::x86::types::FlagFx::None,
                        switch_targets: Vec::new(),
                    });
                    diagnostics.push(format!("decode gap in function {start:#x}: {error}"));
                    break;
                }
            };
            let Some(next_va) = va.checked_add(insn.size).filter(|&end| end <= section_end) else {
                if spec {
                    poisoned = true;
                    break;
                }
                return Err(format!(
                    "instruction {va:#x} crosses executable section boundary"
                ));
            };
            va = next_va;
            let mnem = insn.mnem;
            let rel = insn.rel;
            insns.push(insn);
            if matches!(mnem, Mnem::Ret | Mnem::Int3 | Mnem::Ud2) {
                break;
            }
            if mnem == Mnem::Jcc || mnem == Mnem::Jcxz {
                // a conditional jump always ends its block: both paths become blocks
                queue.push((rel.unwrap(), false));
                succs.push(rel.unwrap());
                succs.push(va);
                queue.push((va, false));
                break;
            }
            if mnem == Mnem::Jmp {
                if let Some(target) = rel {
                    queue.push((target, false));
                } else {
                    // indirect jmp — switch jump table
                    let targets = match text {
                        Some(t) => {
                            let ins = insns.last_mut().unwrap();
                            let targets = switch_targets(image, base, ins, t);
                            ins.switch_targets = targets.clone();
                            targets
                        }
                        None => {
                            insns.last_mut().unwrap().switch_targets = vec![];
                            vec![]
                        }
                    };
                    for t in targets {
                        queue.push((t, true));
                        succs.push(t);
                    }
                }
                break;
            }
        }
        if poisoned {
            continue;
        }
        if insns.is_empty() {
            continue;
        }
        let last = insns.last().unwrap();
        let ends_flow = last.mnem == Mnem::Ret
            || last.mnem == Mnem::Int3
            || last.mnem == Mnem::Ud2
            || last.mnem == Mnem::Jcc
            || last.mnem == Mnem::Jcxz
            || last.mnem == Mnem::Jmp;
        if !ends_flow {
            succs.push(va);
            queue.push((va, false));
        }
        if last.mnem == Mnem::Jcc || last.mnem == Mnem::Jcxz {
            succs.push(last.rel.unwrap());
        }
        index.insert(start, blocks.len());
        blocks.push(Block {
            va: start,
            insns,
            succs,
        });
    }
    Ok((blocks, index))
}

#[cfg(test)]
#[path = "../../tests/unit/translate/blocks.rs"]
mod tests;
