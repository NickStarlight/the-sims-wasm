//! Bounded Windows-1252 conversions, including NUL-terminated sizing calls.
use crate::{Error, GuestMemory, Process, Result};
mod composite;
mod legacy;
pub(super) use composite::valid_flags as valid_wide_flags;

const CP1252: [u16; 32] = [
    0x20ac, 0x81, 0x201a, 0x192, 0x201e, 0x2026, 0x2020, 0x2021, 0x2c6, 0x2030, 0x160, 0x2039,
    0x152, 0x8d, 0x17d, 0x8f, 0x90, 0x2018, 0x2019, 0x201c, 0x201d, 0x2022, 0x2013, 0x2014, 0x2dc,
    0x2122, 0x161, 0x203a, 0x153, 0x9d, 0x17e, 0x178,
];

pub(super) fn decode_byte(byte: u8) -> u16 {
    if (0x80..=0x9f).contains(&byte) {
        CP1252[usize::from(byte - 0x80)]
    } else {
        u16::from(byte)
    }
}

pub(super) fn encode_unit(unit: u16) -> Option<u8> {
    if unit < 0x80 || (0xa0..=0xff).contains(&unit) {
        Some(unit as u8)
    } else {
        CP1252
            .iter()
            .position(|value| *value == unit)
            .map(|index| index as u8 + 0x80)
    }
}

/// Bulk-read bounded text, falling back to one code unit at mapping boundaries.
/// A terminated result includes its NUL. Never demand readable bytes beyond it.
fn read_units<M: GuestMemory>(
    process: &Process<M>,
    address: u32,
    count: u32,
    wide: bool,
    terminated: bool,
) -> Result<Vec<u16>> {
    if count > 1_048_576 {
        return Err(Error::InvalidRange);
    }
    let width = if wide { 2 } else { 1 };
    let mut units = Vec::new();
    while units.len() < count as usize {
        let current = address
            .checked_add((units.len() * width) as u32)
            .ok_or(Error::InvalidRange)?;
        let mut block = [0; 256];
        let requested = block.len().min((count as usize - units.len()) * width);
        let length = if process.read(current, &mut block[..requested]).is_ok() {
            requested
        } else {
            process.read(current, &mut block[..width])?;
            width
        };
        for bytes in block[..length].chunks_exact(width) {
            let unit = if wide {
                u16::from_le_bytes([bytes[0], bytes[1]])
            } else {
                bytes[0] as u16
            };
            units.push(unit);
            if terminated && unit == 0 {
                return Ok(units);
            }
        }
    }
    if terminated {
        Err(Error::Host("unterminated conversion input".into()))
    } else {
        Ok(units)
    }
}

pub(super) fn convert<M: GuestMemory>(
    process: &mut Process<M>,
    args: &[u32],
    wide: bool,
) -> Result<u32> {
    if !matches!(args[0], 0 | 1252) {
        return Err(Error::Host(
            "text conversion currently supports Windows-1252".into(),
        ));
    }
    // CP1252 maps each byte directly to its precomposed Unicode character.
    // MB_PRECOMPOSED is the default; every byte has a mapping, including controls,
    // so MB_ERR_INVALID_CHARS cannot encounter an invalid byte sequence here.
    if (wide && !composite::valid_flags(args[1])) || (!wide && args[1] & !0x09 != 0) {
        return Err(Error::Host(format!(
            "unsupported {} conversion flags {:#x}",
            if wide { "wide-to-ANSI" } else { "ANSI-to-wide" },
            args[1]
        )));
    }
    let terminated = args[3] == u32::MAX;
    if !terminated && (args[3] == 0 || args[3] > 1_048_576) {
        return Err(Error::InvalidRange);
    }
    let units = read_units(
        process,
        args[2],
        if terminated { 1_048_576 } else { args[3] },
        wide,
        terminated,
    )?;
    let default = if wide && args[6] != 0 {
        let mut byte = [0];
        process.read(args[6], &mut byte)?;
        byte[0]
    } else {
        b'?'
    };
    let (output, used_default) = if wide {
        composite::encode(&units, args[1], default)
    } else {
        (
            units
                .into_iter()
                .flat_map(|unit| decode_byte(unit as u8).to_le_bytes())
                .collect(),
            false,
        )
    };
    let required = output.len() as u32 / if wide { 1 } else { 2 };
    if args[5] != 0 {
        if args[5] < required {
            return Ok(0);
        }
        process.write(args[4], &output)?;
    }
    if wide && args[7] != 0 {
        process.write(args[7], &u32::from(used_default).to_le_bytes())?;
    }
    Ok(required)
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/text/bulk.rs"]
mod bulk_tests;
