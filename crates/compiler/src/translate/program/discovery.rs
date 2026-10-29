//! Worklist discovery, with decode-gap filtering for speculative roots.
use super::{images, model::ModuleImage, roots};
use crate::translate::blocks::{split_blocks, Block};
use crate::x86::types::Mnem;
use crate::{guestmem::GuestMem, pe::PeInfo};
use std::collections::{BTreeMap, HashMap, HashSet};

pub(super) type Functions = HashMap<u32, (Vec<Block>, HashMap<u32, usize>)>;

fn initial_roots(
    info: &PeInfo,
    modules: &[ModuleImage],
    entry: u32,
    extra: &[u32],
    diagnostics: &mut Vec<String>,
) -> Vec<u32> {
    let mut roots = Vec::new();
    for &va in extra {
        if images::in_image(info, modules, va) {
            roots.push(va);
        } else {
            diagnostics.push(format!(
                "skipped demand root {va:#x}: outside every mapped image"
            ));
        }
    }
    roots.push(entry);
    roots
}

pub(super) fn discover(
    image: &GuestMem,
    base: u32,
    entry: u32,
    info: &PeInfo,
    extra: &[u32],
    modules: &[ModuleImage],
    diagnostics: &mut Vec<String>,
) -> Functions {
    let mut functions = Functions::new();
    let mut confirmed: HashSet<u32> = extra
        .iter()
        .copied()
        .chain(std::iter::once(entry))
        .collect();
    let code = images::executable_ranges(info, modules);
    let mut worklist = initial_roots(info, modules, entry, extra, diagnostics);
    let mut speculative = Vec::new();
    roots::seed_data(image, base, info, modules, &mut speculative);
    let mut boundaries = BTreeMap::<u32, u32>::new();
    while let Some(address) = worklist.pop().or_else(|| speculative.pop()) {
        if functions.contains_key(&address) {
            continue;
        }
        // Numeric data can happen to look like a code pointer. Establish known
        // instruction boundaries from entry points and their calls first, then
        // discard inferred callbacks which start inside an existing instruction.
        // Later speculative switch paths do not invalidate a valid entry point.
        if !confirmed.contains(&address) {
            if let Some((&known, &size)) = boundaries.range(..address).next_back() {
                if known + size > address {
                    diagnostics.push(format!(
                        "skipped function {address:#x}: speculative root conflicts with instruction {known:#x}"
                    ));
                    continue;
                }
            }
        }
        let (blocks, index) = match split_blocks(
            image,
            base,
            address,
            images::executable_span(info, modules, address),
            &code,
            diagnostics,
        ) {
            Ok(decoded) => decoded,
            Err(error) => {
                diagnostics.push(format!("skipped function {address:#x}: {error}"));
                continue;
            }
        };
        let has_gap = blocks
            .iter()
            .flat_map(|b| &b.insns)
            .any(|i| i.mnem == Mnem::Ud2);
        if has_gap && !confirmed.contains(&address) {
            diagnostics.push(format!(
                "skipped function {address:#x}: decode gap (false root)"
            ));
            continue;
        }
        for insn in blocks.iter().flat_map(|b| &b.insns) {
            if confirmed.contains(&address) {
                boundaries.insert(insn.va, insn.size);
            }
            if let Some(root) = roots::instruction_root(insn, &code) {
                if insn.mnem == Mnem::Call && confirmed.contains(&address) {
                    confirmed.insert(root);
                    worklist.push(root);
                } else {
                    speculative.push(root);
                }
            }
        }
        functions.insert(address, (blocks, index));
    }
    functions
}
