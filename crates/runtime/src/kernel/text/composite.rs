//! CP1252 composite conversion uses Unicode's composition data, with Windows fallback policy.
use super::encode_unit;
use unicode_normalization::char::{compose, is_combining_mark};

const COMPOSITE: u32 = 0x200;
const DISCARD: u32 = 0x10;
const SEPARATE: u32 = 0x20;
const DEFAULT: u32 = 0x40;
const NO_BEST_FIT: u32 = 0x400;

pub(crate) fn valid_flags(flags: u32) -> bool {
    let policy = flags & (DISCARD | SEPARATE | DEFAULT);
    flags & !(COMPOSITE | DISCARD | SEPARATE | DEFAULT | NO_BEST_FIT) == 0
        && policy.count_ones() <= 1
        && (policy == 0 || flags & COMPOSITE != 0)
}

pub(super) fn encode(units: &[u16], flags: u32, default: u8) -> (Vec<u8>, bool) {
    let mut output = Vec::new();
    let mut used_default = false;
    let mut pending = None;
    for ch in char::decode_utf16(units.iter().copied())
        .map(|ch| ch.unwrap_or(char::REPLACEMENT_CHARACTER))
    {
        if flags & COMPOSITE != 0 && is_combining_mark(ch) {
            if let Some(base) = pending {
                if let Some(composed) = compose(base, ch).filter(|ch| encoded(*ch).is_some()) {
                    pending = Some(composed);
                    continue;
                }
                if flags & DISCARD != 0 {
                    continue;
                }
                if flags & DEFAULT != 0 {
                    pending = None;
                    output.push(default);
                    used_default = true;
                    continue;
                }
            } else if flags & DISCARD != 0 {
                continue;
            }
        }
        if let Some(previous) = pending.replace(ch) {
            push(&mut output, previous, default, &mut used_default);
        }
    }
    if let Some(previous) = pending {
        push(&mut output, previous, default, &mut used_default);
    }
    (output, used_default)
}

fn encoded(ch: char) -> Option<u8> {
    u16::try_from(u32::from(ch)).ok().and_then(encode_unit)
}
fn push(output: &mut Vec<u8>, ch: char, default: u8, used_default: &mut bool) {
    output.push(encoded(ch).unwrap_or_else(|| {
        *used_default = true;
        default
    }));
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/text/composite.rs"]
mod tests;
