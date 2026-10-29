//! Material/light payload contracts ported from the working COM adapter and Rust resources.rs.
use super::*;
#[derive(Clone)]
pub(super) struct Material {
    pub bytes: [u8; 80],
    pub handle: u32,
}
#[derive(Clone)]
pub(super) struct Light {
    pub bytes: [u8; 80],
    pub viewport: u32,
}
impl Material {
    pub fn new(handle: u32) -> Self {
        let mut bytes = [0; 80];
        bytes[..4].copy_from_slice(&80u32.to_le_bytes());
        Self { bytes, handle }
    }
}
impl Default for Light {
    fn default() -> Self {
        let mut bytes = [0; 80];
        bytes[..4].copy_from_slice(&80u32.to_le_bytes());
        Self { bytes, viewport: 0 }
    }
}
pub(super) fn transfer<M: GuestMemory>(
    p: &mut Process<M>,
    bytes: &mut [u8; 80],
    address: u32,
    set: bool,
    light: bool,
) -> Result<u32> {
    if address == 0 {
        return Ok(INVALID);
    }
    let size = read_u32(p, address)? as usize;
    if size != 80 && (!light || size != 76) {
        return Ok(INVALID);
    }
    if set {
        let mut payload = [0; 80];
        p.read(address, &mut payload[..size])?;
        if light
            && (!(1..=5).contains(&word(&payload, 4))
                || (0..17).any(|i| !float(&payload, 8 + i * 4).is_finite()))
        {
            return Ok(INVALID);
        }
        if light && size == 76 {
            payload[76..80].copy_from_slice(&1u32.to_le_bytes());
        }
        *bytes = payload;
    } else {
        let mut payload = *bytes;
        if light {
            payload[..4].copy_from_slice(&(size as u32).to_le_bytes());
        }
        p.write(address, &payload[..size])?;
    }
    Ok(0)
}
pub(super) fn material_color(material: &Material) -> u32 {
    let channel =
        |offset| ((float(&material.bytes, offset).clamp(0., 1.) * 255. + 0.5).floor()) as u32;
    channel(16) << 24 | channel(4) << 16 | channel(8) << 8 | channel(12)
}
pub(super) fn caps(software: bool, size: usize) -> Vec<u8> {
    let mut bytes = vec![0; size];
    bytes[..4].copy_from_slice(&(size as u32).to_le_bytes());
    if software {
        for (offset, value) in [
            (4, 0x7ffu32),
            (8, 2),
            (12, 1),
            (16, 8),
            (28, 16),
            (44, 56),
            (100, 56),
            (104, 0x70),
            (108, 0x10),
            (112, 0xff),
            (116, 0x7ff),
            (120, 0x7ff),
            (124, 0xff),
            (128, 0x4008),
            // D3DPTEXTURECAPS_PERSPECTIVE: raster::shade interpolates u*rhw
            // and v*rhw, then divides by the interpolated reciprocal depth.
            (132, 1),
            (156, 0x500),
            (160, 0x400),
            (168, 65535),
        ] {
            if offset + 4 <= size {
                bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
            }
        }
    }
    bytes
}
pub(super) fn word(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
pub(super) fn float(bytes: &[u8], offset: usize) -> f64 {
    f32::from_bits(word(bytes, offset)) as f64
}
