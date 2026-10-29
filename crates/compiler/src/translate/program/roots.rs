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
