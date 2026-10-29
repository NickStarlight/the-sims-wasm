//! Windows ANSI formatting reads guest varargs explicitly; no host pointers or libc ABI cross over.
use super::*;

const LIMIT: usize = 1023;

impl Kernel {
    pub(super) fn wsprintf<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<u32> {
        let format = string(process, args[1], false, 65536)?;
        let mut output = Vec::new();
        let mut cursor = 0;
        let mut stack = args[2];
        while cursor < format.len() && output.len() <= LIMIT {
            if format[cursor] != b'%' {
                output.push(format[cursor]);
                cursor += 1;
                continue;
            }
            cursor += 1;
            let field = Field::parse(&format, &mut cursor)?;
            if field.kind == b'%' {
                output.push(b'%');
                continue;
            }
            let mut bytes = [0; 4];
            process.read(stack, &mut bytes)?;
            stack = stack.checked_add(4).ok_or(Error::InvalidRange)?;
            let value = u32::from_le_bytes(bytes);
            let rendered = field.render(process, value)?;
            let count = (LIMIT + 1 - output.len()).min(rendered.len());
            output.extend_from_slice(&rendered[..count]);
        }
        if output.len() > LIMIT {
            output.truncate(LIMIT);
            self.fail(122);
        }
        let length = output.len() as u32;
        output.push(0);
        process.write(args[0], &output)?;
        Ok(length)
    }
}

#[derive(Default)]
struct Field {
    left: bool,
    alternate: bool,
    zero: bool,
    width: usize,
    precision: Option<usize>,
    modifier: u8,
    kind: u8,
}
impl Field {
    fn parse(format: &[u8], cursor: &mut usize) -> Result<Self> {
        let mut field = Self::default();
        while let Some(&flag) = format.get(*cursor) {
            match flag {
                b'-' => field.left = true,
                b'#' => field.alternate = true,
                b'0' => field.zero = true,
                _ => break,
            }
            *cursor += 1;
        }
        field.width = number(format, cursor);
        if format.get(*cursor) == Some(&b'.') {
            *cursor += 1;
            field.precision = Some(number(format, cursor));
        }
        if matches!(format.get(*cursor), Some(b'h' | b'l' | b'w' | b'I')) {
            field.modifier = format[*cursor];
            *cursor += 1;
        }
        field.kind = *format
            .get(*cursor)
            .ok_or_else(|| Error::Host("unterminated wsprintf format".into()))?;
        *cursor += 1;
        if !matches!(
            field.kind,
            b'%' | b'c' | b'C' | b's' | b'S' | b'd' | b'i' | b'u' | b'x' | b'X' | b'p'
        ) {
            return Err(Error::Host(format!(
                "unsupported wsprintf conversion {}",
                field.kind as char
            )));
        }
        Ok(field)
    }

    fn render<M: GuestMemory>(&self, process: &Process<M>, value: u32) -> Result<Vec<u8>> {
        let numeric = matches!(self.kind, b'd' | b'i' | b'u' | b'x' | b'X' | b'p');
        let wide = match self.modifier {
            b'h' => false,
            b'l' | b'w' => true,
            _ => matches!(self.kind, b'C' | b'S'),
        };
        let mut prefix = Vec::new();
        let mut content = match self.kind {
            b's' | b'S' => string(
                process,
                value,
                wide,
                self.precision.unwrap_or(LIMIT + 1).min(LIMIT + 1),
            )?,
            b'c' | b'C' => {
                let byte = if wide {
                    text::encode_unit(value as u16).unwrap_or(b'?')
                } else {
                    value as u8
                };
                if byte == 0 {
                    Vec::new()
                } else {
                    vec![byte]
                }
            }
            _ => {
                let value = if self.modifier == b'h' {
                    value & 0xffff
                } else {
                    value
                };
                let signed = matches!(self.kind, b'd' | b'i');
                let signed_value = if self.modifier == b'h' {
                    i64::from(value as i16)
                } else {
                    i64::from(value as i32)
                };
                let magnitude = if signed {
                    signed_value.unsigned_abs()
                } else {
                    u64::from(value)
                };
                if signed && signed_value < 0 {
                    prefix.push(b'-');
                }
                if self.alternate && matches!(self.kind, b'x' | b'X') && value != 0 {
                    prefix.extend_from_slice(if self.kind == b'X' { b"0X" } else { b"0x" });
                }
                let digits = match self.kind {
                    b'x' => format!("{magnitude:x}"),
                    b'X' | b'p' => format!("{magnitude:X}"),
                    _ => magnitude.to_string(),
                };
                let precision = if self.kind == b'p' {
                    8
                } else {
                    self.precision.unwrap_or(1).max(1)
                };
                let mut bytes = vec![b'0'; precision.saturating_sub(digits.len())];
                bytes.extend_from_slice(digits.as_bytes());
                bytes
            }
        };
        let padding = self.width.saturating_sub(prefix.len() + content.len());
        let mut output = Vec::new();
        if !self.left && (!self.zero || !numeric) {
            output.resize(padding, b' ');
        }
        output.append(&mut prefix);
        if !self.left && self.zero && numeric {
            output.resize(output.len() + padding, b'0');
        }
        output.append(&mut content);
        if self.left {
            output.resize(output.len() + padding, b' ');
        }
        Ok(output)
    }
}

fn number(format: &[u8], cursor: &mut usize) -> usize {
    let mut value = 0usize;
    while let Some(digit @ b'0'..=b'9') = format.get(*cursor) {
        value = (value * 10 + usize::from(digit - b'0')).min(LIMIT + 1);
        *cursor += 1;
    }
    value
}

fn string<M: GuestMemory>(
    process: &Process<M>,
    address: u32,
    wide: bool,
    limit: usize,
) -> Result<Vec<u8>> {
    if address == 0 {
        return Ok(b"(null)"[..limit.min(6)].to_vec());
    }
    let mut output = Vec::new();
    let mut cursor = address;
    for _ in 0..limit {
        let mut bytes = [0; 2];
        let size = if wide { 2 } else { 1 };
        process.read(cursor, &mut bytes[..size])?;
        let unit = u16::from_le_bytes(bytes);
        if unit == 0 {
            return Ok(output);
        }
        output.push(if wide {
            text::encode_unit(unit).unwrap_or(b'?')
        } else {
            bytes[0]
        });
        cursor = cursor.checked_add(size as u32).ok_or(Error::InvalidRange)?;
    }
    Ok(output)
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/formatting.rs"]
mod tests;
