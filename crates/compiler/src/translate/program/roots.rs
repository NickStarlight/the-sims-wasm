//! Speculative callback roots from data pointers and decoded instructions.
use super::images::{absolute, code_ranges, contains, Range};
use super::model::ModuleImage;
use crate::x86::types::{Insn, Mnem, Operand};
use crate::{guestmem::GuestMem, pe::PeInfo};

fn scan_data(
    image: &GuestMem,
    info: &PeInfo,
    ranges: &[Range],
    address: impl Fn(u32) -> u32,
    roots: &mut Vec<u32>,
) {
    for section in &info.sections {
        if section.characteristics & 0x2000_0000 != 0 || section.name == ".rsrc" {
            continue;
        }
        let mut offset = 0;
        while offset + 4 <= section.size_of_raw_data {
            let value = image.u32_le((address(section.va) + offset) as usize);
            if contains(ranges, value) {
                roots.push(value);
            }
            offset += 4;
        }
    }
}

pub(super) fn seed_data(
    image: &GuestMem,
    base: u32,
    info: &PeInfo,
    modules: &[ModuleImage],
    roots: &mut Vec<u32>,
) {
    let code = code_ranges(info, |va| va);
    if !code.is_empty() {
        scan_data(image, info, &code, |va| va.wrapping_sub(base), roots);
    }
    for module in modules {
        let code = code_ranges(module.info, |va| absolute(module, va));
        if !code.is_empty() {
            scan_data(image, module.info, &code, |va| absolute(module, va), roots);
        }
    }
}

/// Only immediate callbacks and pointer-width memory stores are speculative roots.
/// Register constants and narrow writes must never become function addresses.
pub(super) fn instruction_root(insn: &Insn, code: &[Range]) -> Option<u32> {
    if insn.mnem == Mnem::Call {
        return insn.rel;
    }
    let pointer = match (insn.mnem, insn.ops.as_slice()) {
        (Mnem::Push, [Operand::Imm(value), ..]) => *value as u32,
        (Mnem::Mov, [Operand::Mem(dst), Operand::Imm(value)]) if dst.size == 4 => *value as u32,
        _ => return None,
    };
    contains(code, pointer).then_some(pointer)
}

/// An x86 MSVC EH prologue registers incoming EAX at SEH frame+4.
/// Keep ordinary register constants out of discovery: require the complete
/// address-independent helper body and an adjacent direct call to that helper.
pub(super) fn registered_handler(
    image: &GuestMem,
    base: u32,
    pair: &[Insn],
    code: &[Range],
) -> Option<u32> {
    let [value, call] = pair else {
        return None;
    };
    if value.mnem != Mnem::Mov
        || call.mnem != Mnem::Call
        || value.va.checked_add(value.size)? != call.va
    {
        return None;
    }
    let [Operand::Reg { r: 0, size: 4 }, Operand::Imm(handler)] = value.ops.as_slice() else {
        return None;
    };
    let handler = *handler as u32;
    if !contains(code, handler) {
        return None;
    }
    let helper = call.rel?;
    const PROLOGUE: &[u8] = &[
        0x6a, 0xff, 0x50, 0x64, 0xa1, 0, 0, 0, 0, 0x50, 0x8b, 0x44, 0x24, 0x0c, 0x64, 0x89, 0x25,
        0, 0, 0, 0, 0x89, 0x6c, 0x24, 0x0c, 0x8d, 0x6c, 0x24, 0x0c, 0x50, 0xc3,
    ];
    let end = helper.checked_add(PROLOGUE.len() as u32)?;
    if !code.iter().any(|&(lo, hi)| helper >= lo && end <= hi) {
        return None;
    }
    let offset = helper.checked_sub(base)? as usize;
    PROLOGUE
        .iter()
        .enumerate()
        .all(|(i, &byte)| image.get(offset + i) == Some(byte))
        .then_some(handler)
}
