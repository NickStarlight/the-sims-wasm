//! Specialize the supported executable's vendor dispatch for our fixed AMD CPU.
//!
//! These are decoded control-flow changes, not patches to the mapped guest image.
//! Shared x86/x87 lowering remains available, including exact integer x87 copies.
use crate::{
    guestmem::GuestMem,
    translate::blocks::Block,
    x86::types::{FlagFx, Insn, Mnem, Operand},
};
use std::collections::{HashMap, HashSet};

pub(super) struct Profile;

impl Profile {
    pub(super) fn recognize(image: &GuestMem, base: u32) -> Option<Self> {
        // REA/Ghidra dossiers for Complete Collection executable SHA-256
        // 6e598c22512a24659cbd0e533306abf3129733d67b1d120916b1b6b1581e29f2.
        // Require all affected routines, not just an address or vendor string.
        [
            (0x5b6020, 1006, 0xc15c8e66aa547267u64),
            (0x5a92b0, 112, 0x9708d71f53f9ffd7),
            (0x5a9890, 370, 0x776b62eb1db6756a),
        ]
        .into_iter()
        .all(|(address, length, expected)| {
            fingerprint(image, base, address, length) == Some(expected)
        })
        .then_some(Self)
    }

    pub(super) fn excludes(&self, address: u32) -> bool {
        // The x87 memcpy body has one native entry reference: the Intel arm of
        // 0x5a92b0. Exclude aliases/interior callback roots as well as its entry.
        [
            (0x5a9890, 0x5a9a10),
            (0x5a9316, 0x5a931b),
            (0x5b6110, 0x5b612c),
            (0x5b614d, 0x5b618d),
        ]
        .iter()
        .any(|&(start, end)| (start..end).contains(&address))
    }

    pub(super) fn specialize(
        &self,
        entry: u32,
        blocks: &mut Vec<Block>,
        index: &mut HashMap<u32, usize>,
    ) {
        let mut changed = false;
        for block in blocks.iter_mut() {
            for position in 0..block.insns.len() {
                let insn = &mut block.insns[position];
                let target = match insn.va {
                    // Start with AuthenticAMD, bypassing the Intel comparison.
                    0x5b610b => Some(0x5b612c),
                    // Our CPUID vendor always matches; eliminate other vendors.
                    0x5b613f => Some(0x5b6141),
                    // AMD never selects the Intel-family x87 memcpy body.
                    0x5a9314 => Some(0x5a931b),
                    0x5a930d => {
                        insn.ops = vec![Operand::Reg { r: 0, size: 4 }, Operand::Imm(0)];
                        changed = true;
                        None
                    }
                    _ => None,
                };
                if let Some(target) = target {
                    changed = true;
                    *insn = Insn {
                        va: insn.va,
                        size: insn.size,
                        mnem: Mnem::Jmp,
                        rel: Some(target),
                        flags: FlagFx::None,
                        ..Insn::default()
                    };
                    block.insns.truncate(position + 1);
                    block.succs = vec![target];
                    break;
                }
            }
        }
        if !changed {
            return;
        }
        // split_blocks also discovers both original conditional arms and follows
        // tail jumps into memcpy implementations. Remove unreachable blocks before
        // collecting calls/roots, so Intel code cannot return through discovery.
        let mut reached = HashSet::new();
        let mut pending = vec![entry];
        while let Some(address) = pending.pop() {
            if !reached.insert(address) {
                continue;
            }
            let Some(&bi) = index.get(&address) else {
                continue;
            };
            let block = &blocks[bi];
            let last = block.insns.last().unwrap();
            match last.mnem {
                Mnem::Jmp => {
                    if let Some(target) = last.rel {
                        pending.push(target);
                    }
                    pending.extend(&last.switch_targets);
                }
                Mnem::Ret | Mnem::Int3 | Mnem::Ud2 => {}
                _ => pending.extend(&block.succs),
            }
        }
        blocks.retain(|block| reached.contains(&block.va) && !self.excludes(block.va));
        index.clear();
        index.extend(blocks.iter().enumerate().map(|(i, block)| (block.va, i)));
    }
}

fn fingerprint(image: &GuestMem, base: u32, address: u32, length: usize) -> Option<u64> {
    let offset = address.checked_sub(base)? as usize;
    (0..length).try_fold(0xcbf29ce484222325u64, |hash, i| {
        Some((hash ^ u64::from(image.get(offset + i)?)).wrapping_mul(0x100000001b3))
    })
}
