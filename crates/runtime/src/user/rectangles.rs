//! Signed wrapping rectangle operations ported from runtime-rs/src/graphics/rect.rs.
use super::*;
fn empty(rect: [i32; 4]) -> bool {
    rect[0] >= rect[2] || rect[1] >= rect[3]
}
fn read<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<[i32; 4]> {
    Ok(words::<_, 4>(process, address)?.map(|v| v as i32))
}
fn write<M: GuestMemory>(process: &mut Process<M>, address: u32, rect: [i32; 4]) -> Result<()> {
    write_words(process, address, &rect.map(|v| v as u32))
}
pub(super) fn call<M: GuestMemory>(process: &mut Process<M>, name: &str, a: &[u32]) -> Result<u32> {
    if a[0] == 0 {
        return Ok(u32::from(name == "IsRectEmpty"));
    }
    let mut rect = match name {
        "SetRect" => [a[1] as i32, a[2] as i32, a[3] as i32, a[4] as i32],
        "IntersectRect" | "UnionRect" => {
            if a[1] == 0 || a[2] == 0 {
                return Ok(0);
            }
            let left = read(process, a[1])?;
            let right = read(process, a[2])?;
            let rect = if name == "IntersectRect" {
                [
                    left[0].max(right[0]),
                    left[1].max(right[1]),
                    left[2].min(right[2]),
                    left[3].min(right[3]),
                ]
            } else {
                match (empty(left), empty(right)) {
                    (true, true) => [0; 4],
                    (true, false) => right,
                    (false, true) => left,
                    _ => [
                        left[0].min(right[0]),
                        left[1].min(right[1]),
                        left[2].max(right[2]),
                        left[3].max(right[3]),
                    ],
                }
            };
            let valid = !empty(rect);
            write(process, a[0], if valid { rect } else { [0; 4] })?;
            return Ok(u32::from(valid));
        }
        _ => read(process, a[0])?,
    };
    match name {
        "IsRectEmpty" => return Ok(u32::from(empty(rect))),
        "PtInRect" => {
            return Ok(u32::from(
                rect[0] <= a[1] as i32
                    && (a[1] as i32) < rect[2]
                    && rect[1] <= a[2] as i32
                    && (a[2] as i32) < rect[3],
            ))
        }
        "OffsetRect" => {
            for (i, value) in rect.iter_mut().enumerate() {
                *value = value.wrapping_add(a[1 + i % 2] as i32);
            }
        }
        "InflateRect" => {
            rect = [
                rect[0].wrapping_sub(a[1] as i32),
                rect[1].wrapping_sub(a[2] as i32),
                rect[2].wrapping_add(a[1] as i32),
                rect[3].wrapping_add(a[2] as i32),
            ];
        }
        "SetRect" => {}
        _ => return Err(Error::Host(format!("unknown rectangle service {name}"))),
    }
    write(process, a[0], rect)?;
    Ok(1)
}
