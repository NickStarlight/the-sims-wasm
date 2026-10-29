//! DirectDraw packed pixel formats and descriptor encoding.
use super::*;
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Format {
    pub bits: u32,
    pub red: u32,
    pub green: u32,
    pub blue: u32,
}
impl Format {
    pub fn new(bits: u32) -> Option<Self> {
        let (red, green, blue) = match bits {
            8 => (0, 0, 0),
            15 => (0x7c00, 0x3e0, 0x1f),
            16 => (0xf800, 0x7e0, 0x1f),
            24 | 32 => (0xff0000, 0xff00, 0xff),
            _ => return None,
        };
        Some(Self {
            bits: if bits == 15 { 16 } else { bits },
            red,
            green,
            blue,
        })
    }
    pub fn bytes(self) -> u32 {
        self.bits / 8
    }
    pub fn record(self) -> [u8; 32] {
        let mut bytes = [0; 32];
        for (i, value) in [
            32,
            if self.bits == 8 { 0x60 } else { 0x40 },
            0,
            self.bits,
            self.red,
            self.green,
            self.blue,
            0,
        ]
        .iter()
        .enumerate()
        {
            bytes[i * 4..i * 4 + 4].copy_from_slice(&value.to_le_bytes());
        }
        bytes
    }
    pub fn rgba(self, value: u32, palette: Option<&[[u8; 4]; 256]>) -> [u8; 4] {
        if self.bits == 8 {
            return palette.map_or([value as u8, value as u8, value as u8, 255], |p| {
                let c = p[value as usize & 255];
                [c[0], c[1], c[2], 255]
            });
        }
        let channel = |mask: u32| {
            if mask == 0 {
                0
            } else {
                (u64::from((value & mask) >> mask.trailing_zeros()) * 255
                    / u64::from(mask >> mask.trailing_zeros())) as u8
            }
        };
        [
            channel(self.red),
            channel(self.green),
            channel(self.blue),
            255,
        ]
    }
}
// Shared by all processes; 256 KiB replaces repeated per-frame channel scaling.
// Initialize only if an RGB565 primary surface is presented.
pub(super) static RGB565: std::sync::LazyLock<Vec<[u8; 4]>> =
    std::sync::LazyLock::new(|| (0..=u16::MAX as u32).map(rgb565).collect());

// Exact mask scaling, with constant divisors for the common display format.
#[inline]
pub(super) fn rgb565(value: u32) -> [u8; 4] {
    [
        (((value >> 11) & 31) * 255 / 31) as u8,
        (((value >> 5) & 63) * 255 / 63) as u8,
        ((value & 31) * 255 / 31) as u8,
        255,
    ]
}
pub(super) fn read_pixel(surface: &Surface, x: u32, y: u32) -> u32 {
    let offset = (y * surface.pitch + x * surface.format.bytes()) as usize;
    surface.pixels[offset..offset + surface.format.bytes() as usize]
        .iter()
        .enumerate()
        .fold(0, |value, (i, byte)| value | (u32::from(*byte) << (i * 8)))
}
pub(super) fn descriptor(
    surface: &Surface,
    size: u32,
    pointer: u32,
    width: u32,
    height: u32,
) -> Vec<u8> {
    let mut bytes = vec![0; size as usize];
    let mut put =
        |offset: usize, value: u32| bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
    put(0, size);
    let mut flags = 0x100f;
    if surface.primary && surface.back != 0 {
        flags |= 0x10;
        put(20, 1);
    }
    put(8, height);
    put(12, width);
    put(16, surface.pitch);
    put(36, pointer);
    let caps = if surface.primary {
        0x220 | if surface.back != 0 { 0x18 } else { 0 }
    } else if surface.front != 0 {
        0x1c
    } else {
        surface.caps
    };
    put(104, caps);
    for (key, flag, offset) in [
        (surface.source_key, 0x10000, 64),
        (surface.destination_key, 0x4000, 48),
        (surface.source_overlay_key, 0x8000, 56),
        (surface.destination_overlay_key, 0x2000, 40),
    ] {
        if let Some((low, high)) = key {
            flags |= flag;
            put(offset, low);
            put(offset + 4, high);
        }
    }
    put(4, flags);
    bytes[72..104].copy_from_slice(
        &surface
            .pixel_format
            .unwrap_or_else(|| surface.format.record()),
    );
    bytes
}

#[cfg(test)]
#[path = "../../tests/unit/graphics/format.rs"]
mod tests;
