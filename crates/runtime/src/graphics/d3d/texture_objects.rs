//! Texture COM views retain the original image-copy and palette contracts.
use super::*;
impl Graphics {
    pub(super) fn d3d_texture_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let Some(&id) = self.d3d.textures.get(&a[0]) else {
            return Ok(INVALID);
        };
        match name {
            "D3DTexture_Initialize" => Ok(0x88760005),
            "D3DTexture_Unload" => Ok(0),
            "D3DTexture_GetHandle" => {
                if a[2] == 0 {
                    return Ok(INVALID);
                }
                let valid = self.d3d.device_id(a[1]).is_some();
                write_u32(p, a[2], if valid { a[0] } else { 0 })?;
                Ok(if valid { 0 } else { INVALID })
            }
            "D3DTexture_PaletteChanged" => Ok(
                if self.surfaces[&id].format.bits == 8 && a[1] <= 256 && a[2] <= 256 - a[1] {
                    0
                } else {
                    INVALID
                },
            ),
            "D3DTexture_Load" => {
                let Some(source) = self
                    .d3d
                    .textures
                    .get(&a[1])
                    .and_then(|id| self.surfaces.get(id))
                    .cloned()
                else {
                    return Ok(INVALID);
                };
                let target = &self.surfaces[&id];
                if source.width != target.width
                    || source.height != target.height
                    || source.format.bits != target.format.bits
                    || !texture::supported(&source)
                    || !texture::supported(target)
                    || texture::masks(&source) != texture::masks(target)
                {
                    return Ok(INVALID);
                }
                if source.format.bits == 8 {
                    let Some(palette) = self.palettes.get(&source.palette) else {
                        return Ok(NOT_IMPLEMENTED);
                    };
                    let count = palette.count;
                    let entries = palette.entries;
                    let Some(destination) = self.palettes.get_mut(&target.palette) else {
                        return Ok(NOT_IMPLEMENTED);
                    };
                    if count == 0 || count != destination.count {
                        return Ok(NOT_IMPLEMENTED);
                    }
                    destination.entries = entries;
                }
                self.surfaces
                    .get_mut(&id)
                    .unwrap()
                    .replace_pixels(source.pixels);
                Ok(0)
            }
            _ => Ok(NOT_IMPLEMENTED),
        }
    }
}
