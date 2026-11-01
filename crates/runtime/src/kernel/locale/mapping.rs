//! Simple Windows Latin case mapping. Sort keys and linguistic transforms need an NLS database.
use super::*;

const MAX_UNITS: u32 = 1_048_576;

impl Kernel {
    pub(in crate::kernel) fn map_string<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<u32> {
        if !matches!(args[0], 0 | 0x400 | 0x800 | 0x409 | 0x7f) {
            return Err(Error::Host(format!(
                "LCMapStringW locale {:#x} is unsupported",
                args[0]
            )));
        }
        if args[1] == 0 || args[1] & 0x300 == 0x300 {
            return Ok(self.fail(1004));
        }
        if !matches!(args[1], 0x100 | 0x200) {
            return Err(Error::Host(format!(
                "LCMapStringW flags {:#x} require unsupported NLS transforms",
                args[1]
            )));
        }
        if args[2] == 0 || args[3] == 0 || (args[5] as i32) < 0 {
            return Ok(self.fail(87));
        }
        let terminated = (args[3] as i32) < 0;
        if !terminated && args[3] > MAX_UNITS {
            return Err(Error::InvalidRange);
        }
        let mut output = Vec::new();
        let mut ended = !terminated;
        for index in 0..if terminated { MAX_UNITS } else { args[3] } {
            let address = args[2].checked_add(index * 2).ok_or(Error::InvalidRange)?;
            let mut bytes = [0; 2];
            process.read(address, &mut bytes)?;
            let unit = u16::from_le_bytes(bytes);
            output.extend(case(unit, args[1] == 0x200)?.to_le_bytes());
            if terminated && unit == 0 {
                ended = true;
                break;
            }
        }
        if !ended {
            return Err(Error::Host("unterminated LCMapStringW input".into()));
        }
        let count = output.len() as u32 / 2;
        if args[5] == 0 {
            return Ok(count);
        }
        if args[4] == 0 || args[5] < count {
            return Ok(self.fail(122));
        }
        // Snapshotting the source above also supports the documented in-place case operation.
        process.write(args[4], &output)?;
        Ok(count)
    }
}

fn case(unit: u16, uppercase: bool) -> Result<u16> {
    if unit > 0xff && text::encode_unit(unit).is_none() {
        return Err(Error::Host(format!(
            "LCMapStringW case mapping outside supported Latin set: U+{unit:04X}"
        )));
    }
    // Simple casing matches legacy Windows: sharp S remains a single character,
    // rather than inheriting Rust's full Unicode uppercase expansion to "SS".
    Ok(if uppercase {
        match unit {
            0x61..=0x7a | 0xe0..=0xf6 | 0xf8..=0xfe => unit - 0x20,
            0xff => 0x178,
            0x153 => 0x152,
            0x161 => 0x160,
            0x17e => 0x17d,
            0xb5 => 0x39c,
            _ => unit,
        }
    } else {
        match unit {
            0x41..=0x5a | 0xc0..=0xd6 | 0xd8..=0xde => unit + 0x20,
            0x178 => 0xff,
            0x152 => 0x153,
            0x160 => 0x161,
            0x17d => 0x17e,
            _ => unit,
        }
    })
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/locale/mapping.rs"]
mod tests;
