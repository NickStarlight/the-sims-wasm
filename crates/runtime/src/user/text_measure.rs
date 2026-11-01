//! Existing DrawTextA measurement contract: 8-pixel columns, 16-pixel lines, no rasterization.
use super::*;
pub(super) fn draw_text<M: GuestMemory>(process: &mut Process<M>, a: &[u32]) -> Result<u32> {
    let count = a[2] as i32;
    if count == 0 {
        return Ok(0);
    }
    if a[1] == 0 {
        return Ok(16);
    }
    let limit = if count == -1 {
        65536
    } else {
        count.max(0) as u32
    };
    if limit > 1_048_576 {
        return Err(Error::InvalidRange);
    }
    let mut text = Vec::new();
    while text.len() < limit as usize {
        let address = a[1]
            .checked_add(text.len() as u32)
            .ok_or(Error::InvalidRange)?;
        let mut bytes = [0; 128];
        let count = bytes.len().min(limit as usize - text.len());
        let count = if process.read(address, &mut bytes[..count]).is_ok() {
            count
        } else {
            process.read(address, &mut bytes[..1])?;
            1
        };
        if let Some(end) = bytes[..count].iter().position(|b| *b == 0) {
            text.extend_from_slice(&bytes[..end]);
            break;
        }
        text.extend_from_slice(&bytes[..count]);
    }
    let single = a[4] & 0x20 != 0;
    let calculate = a[4] & 0x400 != 0;
    let (mut lines, mut widest, mut columns, mut offset) = (1u32, 0u32, 0u32, 0);
    while offset < text.len() {
        let ch = text[offset];
        if !single && matches!(ch, 10 | 13) {
            widest = widest.max(columns);
            if offset + 1 < text.len()
                && text[offset + 1] != ch
                && matches!(text[offset + 1], 10 | 13)
            {
                offset += 1;
            }
            lines += 1;
            columns = 0;
        } else {
            columns += 1;
        }
        offset += 1;
    }
    widest = widest.max(columns);
    let height = if text.is_empty() {
        u32::from(single) * 16
    } else {
        lines * 16
    };
    if calculate {
        let [left, top, _, _] = words::<_, 4>(process, a[3])?;
        write_words(
            process,
            a[3],
            &[
                left,
                top,
                left.wrapping_add(widest * 8),
                top.wrapping_add(height),
            ],
        )?;
    }
    Ok(if text.is_empty() { 16 } else { height })
}
