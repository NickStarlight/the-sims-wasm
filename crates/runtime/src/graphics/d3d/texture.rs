//! Palette/mask sampling and stage-zero combine ported from runtime-rs/src/graphics/texture.rs.
use super::math::pack_color;

pub(super) struct TextureSampler<'a> {
    pub pixels: &'a [u8],
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    /// Red, green, blue, alpha masks.
    pub masks: [u32; 4],
    /// Palette entries retain source RGBA byte order; palette alpha was ignored.
    pub palette: Option<&'a [[u8; 4]]>,
    pub color_key: Option<(u32, u32)>,
    pub address_u: u32,
    pub address_v: u32,
    pub linear: bool,
    pub border: u32,
}
impl TextureSampler<'_> {
    pub fn sample(&self, u: f64, v: f64) -> u32 {
        if !u.is_finite() || !v.is_finite() {
            return 0;
        }
        let x = u * f64::from(self.width);
        let y = v * f64::from(self.height);
        if !self.linear {
            return self.texel(x.floor() as i64, y.floor() as i64);
        }
        let x = x - 0.5;
        let y = y - 0.5;
        let ix = x.floor();
        let iy = y.floor();
        let fx = x - ix;
        let fy = y - iy;
        let (px, py) = (ix as i64, iy as i64);
        let colors = [
            self.texel(px, py),
            self.texel(px.saturating_add(1), py),
            self.texel(px, py.saturating_add(1)),
            self.texel(px.saturating_add(1), py.saturating_add(1)),
        ]
        .map(unpack);
        let weights = [
            (1.0 - fx) * (1.0 - fy),
            fx * (1.0 - fy),
            (1.0 - fx) * fy,
            fx * fy,
        ];
        pack_color(std::array::from_fn(|channel| {
            (0..4).map(|i| colors[i][channel] * weights[i]).sum()
        }))
    }
    fn texel(&self, x: i64, y: i64) -> u32 {
        if self.width == 0 || self.height == 0 {
            return 0;
        }
        let x = coordinate(x, i64::from(self.width), self.address_u);
        let y = coordinate(y, i64::from(self.height), self.address_v);
        if x < 0 || y < 0 {
            return self.border;
        }
        let bytes = (self.bpp >> 3) as usize;
        if !matches!(bytes, 1..=4) {
            return 0;
        }
        let Some(offset) = (y as usize)
            .checked_mul(self.pitch as usize)
            .and_then(|row| {
                (x as usize)
                    .checked_mul(bytes)
                    .and_then(|column| row.checked_add(column))
            })
        else {
            return 0;
        };
        let Some(end) = offset.checked_add(bytes) else {
            return 0;
        };
        let Some(source) = self.pixels.get(offset..end) else {
            return 0;
        };
        let mut value = [0; 4];
        value[..bytes].copy_from_slice(source);
        let pixel = u32::from_le_bytes(value);
        if self
            .color_key
            .is_some_and(|(low, high)| pixel >= low && pixel <= high)
        {
            return 0;
        }
        if self.bpp == 8 {
            let Some(color) = self.palette.and_then(|palette| palette.get(pixel as usize)) else {
                return 0;
            };
            return 0xff00_0000
                | u32::from(color[0]) << 16
                | u32::from(color[1]) << 8
                | u32::from(color[2]);
        }
        channel(pixel, self.masks[3], 255) << 24
            | channel(pixel, self.masks[0], 0) << 16
            | channel(pixel, self.masks[1], 0) << 8
            | channel(pixel, self.masks[2], 0)
    }
}
fn coordinate(x: i64, size: i64, mode: u32) -> i64 {
    match mode {
        3 => x.clamp(0, size - 1),
        4 => {
            if x < 0 || x >= size {
                -1
            } else {
                x
            }
        }
        2 => {
            let value = x.rem_euclid(2 * size);
            if value < size {
                value
            } else {
                2 * size - 1 - value
            }
        }
        _ => x.rem_euclid(size),
    }
}
fn byte(value: f64) -> u32 {
    (value.clamp(0.0, 255.0) + 0.5).floor() as u32
}
fn channel(pixel: u32, mask: u32, empty: u32) -> u32 {
    if mask == 0 {
        return empty;
    }
    let shift = mask.trailing_zeros();
    byte(f64::from((pixel & mask) >> shift) * 255.0 / f64::from(mask >> shift))
}
fn unpack(color: u32) -> [f64; 4] {
    [
        (color >> 16 & 255) as f64,
        (color >> 8 & 255) as f64,
        (color & 255) as f64,
        (color >> 24) as f64,
    ]
}
/// Old packed stage descriptor: color op/arg1/arg2, alpha op/arg1/arg2, texture factor.
pub(super) fn combine(stage: &[u32; 7], diffuse: u32, texture: u32) -> u32 {
    let argument = |index, shift| {
        let selector = stage[index];
        let source = match selector & 15 {
            2 => texture,
            3 => stage[6],
            _ => diffuse,
        };
        let value = ((source >> if selector & 32 != 0 { 24 } else { shift }) & 255) as f64;
        if selector & 16 != 0 {
            255.0 - value
        } else {
            value
        }
    };
    let mut result = 0;
    for shift in [0, 8, 16] {
        result |= byte(operation(
            stage[0],
            argument(1, shift),
            argument(2, shift),
            (texture >> 24) as f64,
        )) << shift;
    }
    result |= if stage[3] == 1 {
        diffuse & 0xff00_0000
    } else {
        byte(operation(
            stage[3],
            argument(4, 24),
            argument(5, 24),
            (texture >> 24) as f64,
        )) << 24
    };
    result
}
fn operation(op: u32, a: f64, b: f64, alpha: f64) -> f64 {
    match op {
        2 => a,
        3 => b,
        4 => a * b / 255.0,
        7 => (a + b).min(255.0),
        13 => (a * alpha + b * (255.0 - alpha)) / 255.0,
        _ => b,
    }
}
#[cfg(test)]
#[path = "../../../tests/unit/graphics/d3d/texture.rs"]
mod tests;

pub(super) fn masks(surface: &super::Surface) -> [u32; 4] {
    surface
        .pixel_format
        .as_ref()
        .map(|p| {
            std::array::from_fn(|i| {
                u32::from_le_bytes(p[16 + i * 4..20 + i * 4].try_into().unwrap())
            })
        })
        .unwrap_or([
            surface.format.red,
            surface.format.green,
            surface.format.blue,
            0,
        ])
}
pub(super) fn supported(surface: &super::Surface) -> bool {
    if surface.width == 0 || surface.height == 0 {
        return false;
    }
    let Some(p) = surface.pixel_format.as_ref() else {
        return matches!(surface.format.bits, 16 | 32);
    };
    let flags = u32::from_le_bytes(p[4..8].try_into().unwrap());
    if flags & 0x20 != 0 {
        return surface.format.bits == 8;
    }
    let values = [
        flags,
        surface.format.bits,
        masks(surface)[0],
        masks(surface)[1],
        masks(surface)[2],
        masks(surface)[3],
    ];
    [
        [0x40, 16, 0xf800, 0x7e0, 0x1f, 0],
        [0x41, 16, 0x7c00, 0x3e0, 0x1f, 0x8000],
        [0x41, 16, 0xf00, 0xf0, 0xf, 0xf000],
        [0x40, 32, 0xff0000, 0xff00, 0xff, 0],
        [0x41, 32, 0xff0000, 0xff00, 0xff, 0xff000000],
    ]
    .contains(&values)
}
