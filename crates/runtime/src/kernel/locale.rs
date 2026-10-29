//! CTYPE1 for ASCII, Latin-1 and Windows-1252 characters used by the CRT.
//! This is not a Windows NLS database: other Unicode ranges and CTYPE2/3 fail
//! explicitly instead of silently inventing Windows-version-specific categories.
//! C1 flags: https://learn.microsoft.com/windows/win32/api/stringapiset/nf-stringapiset-getstringtypew
use super::*;
mod data;
mod information;
mod mapping;
mod profile;

const UPPER: u16 = 0x1;
const LOWER: u16 = 0x2;
const DIGIT: u16 = 0x4;
const SPACE: u16 = 0x8;
const PUNCT: u16 = 0x10;
const CONTROL: u16 = 0x20;
const BLANK: u16 = 0x40;
const HEX: u16 = 0x80;
const ALPHA: u16 = 0x100;
const DEFINED: u16 = 0x200;

fn classify(unit: u16) -> Result<u16> {
    if unit > 0xff && text::encode_unit(unit).is_none() {
        return Err(Error::Host(format!(
            "CTYPE1 classification outside supported Latin character set: U+{unit:04X}"
        )));
    }
    let ch = char::from_u32(u32::from(unit)).ok_or(Error::InvalidRange)?;
    let mut flags = DEFINED;
    for (condition, flag) in [
        (ch.is_uppercase(), UPPER),
        (ch.is_lowercase(), LOWER),
        (ch.is_ascii_digit(), DIGIT),
        (ch.is_whitespace(), SPACE),
        (ch.is_control(), CONTROL),
        (matches!(ch, ' ' | '\t' | '\u{a0}'), BLANK),
        (ch.is_ascii_hexdigit(), HEX),
        (ch.is_alphabetic(), ALPHA),
    ] {
        if condition {
            flags |= flag;
        }
    }
    if !ch.is_alphabetic() && !ch.is_ascii_digit() && !ch.is_whitespace() && !ch.is_control() {
        flags |= PUNCT;
    }
    Ok(flags)
}

impl Kernel {
    pub(super) fn string_type<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let wide = call == Call::GetStringTypeW;
        let args = if wide {
            args
        } else {
            if !matches!(args[0], 0 | 0x400 | 0x800 | 0x409) {
                return Err(Error::Host(
                    "GetStringTypeA supports the English Windows-1252 locale".into(),
                ));
            }
            &args[1..]
        };
        if args[0] != 1 {
            return Err(Error::Host(format!(
                "unsupported character classification type {} (only CTYPE1 implemented)",
                args[0]
            )));
        }
        if args[1] == 0 || args[3] == 0 || args[2] == 0 {
            return Ok(self.fail(87));
        }
        let terminated = args[2] == u32::MAX;
        if !terminated && args[2] > 1_048_576 {
            return Err(Error::InvalidRange);
        }
        let width = if wide { 2 } else { 1 };
        let mut output = Vec::new();
        let mut ended = !terminated;
        for index in 0..if terminated { 1_048_576 } else { args[2] } {
            let address = args[1]
                .checked_add(index * width)
                .ok_or(Error::InvalidRange)?;
            let mut bytes = [0; 2];
            process.read(address, &mut bytes[..width as usize])?;
            let unit = if wide {
                u16::from_le_bytes(bytes)
            } else {
                text::decode_byte(bytes[0])
            };
            output.extend(classify(unit)?.to_le_bytes());
            if terminated && unit == 0 {
                ended = true;
                break;
            }
        }
        if !ended {
            return Err(Error::Host(
                "unterminated character classification input".into(),
            ));
        }
        process.write(args[3], &output)?;
        Ok(1)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/locale.rs"]
mod tests;
