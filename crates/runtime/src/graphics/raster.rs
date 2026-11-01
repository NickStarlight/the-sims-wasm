//! Legacy packed-surface clipping/copy/fill pipeline, adapted to owned pixel slices.
use super::*;
#[derive(Clone, Copy)]
pub(super) struct Rect {
    pub left: i32,
    pub top: i32,
    right: i32,
    bottom: i32,
}
impl Rect {
    pub fn width(self) -> u32 {
        (i64::from(self.right) - i64::from(self.left)).max(0) as u32
    }
    pub fn height(self) -> u32 {
        (i64::from(self.bottom) - i64::from(self.top)).max(0) as u32
    }
    pub fn within(self, w: u32, h: u32) -> bool {
        self.left >= 0
            && self.top >= 0
            && self.right >= self.left
            && self.bottom >= self.top
            && self.right as u32 <= w
            && self.bottom as u32 <= h
    }
    fn clipped(self, s: &Surface) -> Self {
        Self {
            left: self.left.max(0),
            top: self.top.max(0),
            right: self.right.min(s.width as i32),
            bottom: self.bottom.min(s.height as i32),
        }
    }
    fn empty(self) -> bool {
        self.right <= self.left || self.bottom <= self.top
    }
}
pub(super) fn rectangle<M: GuestMemory>(
    p: &Process<M>,
    address: u32,
    w: u32,
    h: u32,
) -> Result<Rect> {
    if address == 0 {
        return Ok(Rect {
            left: 0,
            top: 0,
            right: w as i32,
            bottom: h as i32,
        });
    }
    let mut bytes = [0; 16];
    p.read(address, &mut bytes)?;
    let word = |i| i32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
    Ok(Rect {
        left: word(0),
        top: word(4),
        right: word(8),
        bottom: word(12),
    })
}
impl Graphics {
    pub(super) fn blit<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let fast = name == "Surface_BltFast";
        let strict = name != "Surface_BltBatch";
        let dest = &self.surfaces[&a[0]];
        let source_id = if fast { a[3] } else { a[2] };
        let source = self.surfaces.get(&source_id).cloned();
        let (dr, sr, flags, fx) = if fast {
            let Some(source) = &source else {
                return Ok(INVALID);
            };
            let sr = rectangle(p, a[4], source.width, source.height)?;
            let Some(right) = a[1].checked_add(sr.width()).filter(|&v| v <= dest.width) else {
                return Ok(0x88760096);
            };
            let Some(bottom) = a[2].checked_add(sr.height()).filter(|&v| v <= dest.height) else {
                return Ok(0x88760096);
            };
            let flags =
                if a[5] & 1 != 0 { 0x8000 } else { 0 } | if a[5] & 2 != 0 { 0x2000 } else { 0 };
            (
                Rect {
                    left: a[1] as i32,
                    top: a[2] as i32,
                    right: right as i32,
                    bottom: bottom as i32,
                },
                sr,
                flags,
                0,
            )
        } else {
            let mut dr = rectangle(p, a[1], dest.width, dest.height)?;
            let mut sr = if a[3] == 0 {
                dr
            } else {
                rectangle(p, a[3], 0, 0)?
            };
            if strict && (a[1] != 0 && dr.empty() || a[3] != 0 && sr.empty()) {
                return Ok(0x88760096);
            }
            if !strict && dr.empty() {
                dr = rectangle(p, 0, dest.width, dest.height)?;
            }
            if !strict && sr.empty() {
                sr = dr;
            }
            (dr, sr, a[4], a[5])
        };
        let flags = match blit_flags::validate(p, dest, source.as_ref(), flags, fx)? {
            Ok(flags) => flags,
            Err(error) => return Ok(if fast { 0 } else { error }),
        };
        let fill = flags & (0x400 | 0x2000000) != 0;
        let color = if fill && fx != 0 {
            read_u32(p, fx.checked_add(80).ok_or(Error::InvalidRange)?)?
        } else {
            0
        };
        let source_key = if flags & 0x8000 != 0 {
            source.as_ref().and_then(|s| s.source_key.map(|key| key.0))
        } else if flags & 0x10000 != 0 && fx != 0 {
            Some(read_u32(p, fx.checked_add(92).ok_or(Error::InvalidRange)?)?)
        } else {
            None
        };
        let destination_key = if flags & 0x2000 != 0 {
            dest.destination_key.map(|key| key.0)
        } else {
            None
        };
        let palette = source
            .as_ref()
            .and_then(|s| self.palettes.get(&s.palette))
            .map(|p| p.entries);
        let dr = dr.clipped(dest);
        let sr = if fill {
            dr
        } else {
            sr.clipped(source.as_ref().unwrap())
        };
        let width = dr.width().min(sr.width());
        let height = dr.height().min(sr.height());
        if width == 0 || height == 0 {
            return Ok(0);
        }
        let dest = self.surfaces.get_mut(&a[0]).unwrap();
        copy_pixels(
            dest,
            source.as_ref(),
            (dr, sr),
            (width, height),
            fill.then_some(color),
            (source_key, destination_key),
            palette.as_ref(),
        );
        Ok(0)
    }
    pub(super) fn blit_batch<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        for index in 0..a[2].min(1024) {
            let pointer = a[1].checked_add(index * 20).ok_or(Error::InvalidRange)?;
            let mut bytes = [0; 20];
            p.read(pointer, &mut bytes)?;
            let words: [u32; 5] = std::array::from_fn(|i| {
                u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
            });
            let result = self.blit(
                p,
                "Surface_BltBatch",
                &[a[0], words[0], words[1], words[2], words[3], words[4]],
            )?;
            if result != 0 {
                return Ok(result);
            }
        }
        self.present(a[0])?;
        Ok(0)
    }
}
fn copy_pixels(
    dest: &mut Surface,
    source: Option<&Surface>,
    (dr, sr): (Rect, Rect),
    (width, height): (u32, u32),
    fill: Option<u32>,
    (source_key, destination_key): (Option<u32>, Option<u32>),
    palette: Option<&[[u8; 4]; 256]>,
) {
    // An exact packed-surface copy can retain the source snapshot. Both surfaces
    // detach on later writes, while replace_pixels invalidates the guest copy.
    // Require tight rows so destination padding is never overwritten implicitly.
    // Keep unique destination storage reusable: otherwise sharing can merely move
    // an allocation/copy to the source's next writable Unlock, slowing normal frames.
    if fill.is_none()
        && source_key.is_none()
        && destination_key.is_none()
        && Rc::strong_count(&dest.pixels) > 1
    {
        if let Some(source) = source.filter(|source| {
            source.format == dest.format
                && (dest.width, dest.height) == (width, height)
                && (source.width, source.height) == (width, height)
                && (dr.left, dr.top, sr.left, sr.top) == (0, 0, 0, 0)
                && u64::from(dest.pitch) == u64::from(width) * u64::from(dest.format.bytes())
                && source.pitch == dest.pitch
                && source.pixels.len() as u64 == u64::from(source.pitch) * u64::from(height)
                && dest.pixels.len() == source.pixels.len()
        }) {
            dest.replace_pixels(source.pixels.clone());
            return;
        }
    }
    let bytes = dest.format.bytes() as usize;
    let pitch = dest.pitch;
    let pixels = dest.pixels_mut();
    if let Some(color) = fill {
        // Every row contains the same packed color. Build it once, then use
        // bulk copies without touching the surrounding pixels or row padding.
        // Fill operations intentionally ignore color keys, as in the scalar path.
        let start = dr.top as usize * pitch as usize + dr.left as usize * bytes;
        let len = width as usize * bytes;
        let first = &mut pixels[start..start + len];
        let color = color.to_le_bytes();
        match bytes {
            1 => first.fill(color[0]),
            2 => first.as_chunks_mut::<2>().0.fill([color[0], color[1]]),
            3 => first
                .as_chunks_mut::<3>()
                .0
                .fill([color[0], color[1], color[2]]),
            4 => first.as_chunks_mut::<4>().0.fill(color),
            _ => unreachable!("surface format must contain one to four bytes"),
        }
        for y in 1..height as usize {
            pixels.copy_within(start..start + len, start + y * pitch as usize);
        }
        return;
    }
    // Most game blits copy packed pixels without format conversion. Keep row
    // addressing and format dispatch outside the pixel loop. The source Rc is
    // still a snapshot, including when a surface is blitted onto itself.
    let source = source.unwrap();
    if source.format.bytes() as usize == bytes
        && (bytes == 2 || source_key.is_none() && destination_key.is_none())
    {
        for y in 0..height {
            let dst = (dr.top as usize + y as usize) * pitch as usize + dr.left as usize * bytes;
            let src =
                (sr.top as usize + y as usize) * source.pitch as usize + sr.left as usize * bytes;
            let len = width as usize * bytes;
            let destination = &mut pixels[dst..dst + len];
            let source = &source.pixels[src..src + len];
            if source_key.is_none() && destination_key.is_none() {
                destination.copy_from_slice(source);
            } else {
                for (dst, src) in destination
                    .as_chunks_mut::<2>()
                    .0
                    .iter_mut()
                    .zip(source.as_chunks::<2>().0)
                {
                    let value = u32::from(u16::from_le_bytes([src[0], src[1]]));
                    if source_key == Some(value)
                        || destination_key.is_some_and(|key| {
                            u32::from(u16::from_le_bytes([dst[0], dst[1]])) != key
                        })
                    {
                        continue;
                    }
                    dst.copy_from_slice(src);
                }
            }
        }
        return;
    }
    for y in 0..height {
        for x in 0..width {
            let offset =
                ((dr.top as u32 + y) * pitch + (dr.left as u32 + x) * bytes as u32) as usize;
            if destination_key.is_some_and(|key| {
                let old = pixels[offset..offset + bytes]
                    .iter()
                    .enumerate()
                    .fold(0, |value, (i, &byte)| value | u32::from(byte) << (i * 8));
                old != key
            }) {
                continue;
            }
            let value = format::read_pixel(source, sr.left as u32 + x, sr.top as u32 + y);
            if source_key == Some(value) {
                continue;
            }
            let value = convert(value, source.format.bytes(), bytes as u32, palette);
            pixels[offset..offset + bytes].copy_from_slice(&value.to_le_bytes()[..bytes]);
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/graphics/raster/sharing.rs"]
mod sharing_tests;
fn convert(value: u32, source: u32, dest: u32, palette: Option<&[[u8; 4]; 256]>) -> u32 {
    match (source, dest) {
        (1, 2) | (1, 4) => {
            let index = value as u8;
            let [r, g, b, _] = palette.map_or([index, index, index, 0], |p| p[index as usize]);
            let (r, g, b) = (u32::from(r), u32::from(g), u32::from(b));
            if dest == 2 {
                (r >> 3) << 11 | (g >> 2) << 5 | b >> 3
            } else {
                0xff000000 | r << 16 | g << 8 | b
            }
        }
        (2, 4) => 0xff000000 | (value & 0xf800) << 8 | (value & 0x07e0) << 5 | (value & 0x1f) << 3,
        (4, 2) => (value >> 8) & 0xf800 | (value >> 5) & 0x07e0 | (value >> 3) & 0x1f,
        _ => value,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/graphics/raster.rs"]
mod tests;
