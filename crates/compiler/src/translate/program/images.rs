//! Address validation and executable ranges for the main image and mapped DLLs.
use super::model::ModuleImage;
use crate::pe::PeInfo;

pub(super) type Range = (u32, u32);

pub(super) fn contains(ranges: &[Range], address: u32) -> bool {
    ranges.iter().any(|&(lo, hi)| address >= lo && address < hi)
}

pub(super) fn absolute(module: &ModuleImage, preferred: u32) -> u32 {
    module
        .base
        .wrapping_add(preferred.wrapping_sub(module.info.image_base))
}

pub(super) fn code_ranges(info: &PeInfo, relocate: impl Fn(u32) -> u32) -> Vec<Range> {
    info.sections
        .iter()
        .filter(|s| {
            s.characteristics & 0x2000_0000 != 0 && s.virtual_size.max(s.size_of_raw_data) > 0
        })
        .map(|s| {
            let start = relocate(s.va);
            (
                start,
                start.wrapping_add(s.virtual_size.max(s.size_of_raw_data)),
            )
        })
        .collect()
}

pub(super) fn executable_ranges(info: &PeInfo, modules: &[ModuleImage]) -> Vec<Range> {
    let mut ranges = code_ranges(info, |va| va);
    for module in modules {
        ranges.extend(code_ranges(module.info, |va| absolute(module, va)));
    }
    ranges
}

pub(super) fn in_image(info: &PeInfo, modules: &[ModuleImage], va: u32) -> bool {
    contains(
        &[(
            info.image_base,
            info.image_base.wrapping_add(info.size_of_image),
        )],
        va,
    ) || modules
        .iter()
        .any(|m| contains(&[(m.base, m.base.wrapping_add(m.size_of_image))], va))
}

/// Switch-table decoding uses the containing image's executable envelope.
/// Callback validation uses exact ranges instead, excluding gaps between sections.
pub(super) fn executable_span(info: &PeInfo, modules: &[ModuleImage], va: u32) -> Option<Range> {
    for module in modules {
        if contains(
            &[(module.base, module.base.wrapping_add(module.size_of_image))],
            va,
        ) {
            let ranges = code_ranges(module.info, |va| absolute(module, va));
            return Some((
                ranges.iter().map(|r| r.0).min()?,
                ranges.iter().map(|r| r.1).max()?,
            ));
        }
    }
    let ranges = code_ranges(info, |va| va);
    Some((
        ranges.iter().map(|r| r.0).min()?,
        ranges.iter().map(|r| r.1).max()?,
    ))
}
