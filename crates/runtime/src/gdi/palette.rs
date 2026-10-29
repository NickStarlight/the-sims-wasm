//! Logical palettes are owned GDI objects; truecolor displays have no system palette to remap.
use super::{objects::Object, Gdi};
pub(super) const DEFAULT_PALETTE: u32 = 0x0e00_000f;
pub(super) fn default_entries() -> Vec<[u8; 4]> {
    [
        [0, 0, 0],
        [128, 0, 0],
        [0, 128, 0],
        [128, 128, 0],
        [0, 0, 128],
        [128, 0, 128],
        [0, 128, 128],
        [192, 192, 192],
        [192, 220, 192],
        [166, 202, 240],
        [255, 251, 240],
        [160, 160, 164],
        [128, 128, 128],
        [255, 0, 0],
        [0, 255, 0],
        [255, 255, 0],
        [0, 0, 255],
        [255, 0, 255],
        [0, 255, 255],
        [255, 255, 255],
    ]
    .into_iter()
    .map(|[r, g, b]| [r, g, b, 0])
    .collect()
}
impl Gdi {
    pub fn create_palette(&mut self, entries: Vec<[u8; 4]>) -> std::result::Result<u32, u32> {
        if entries.is_empty() || entries.len() > 256 {
            return Err(87);
        }
        if self.objects.len() >= 4096 {
            return Err(8);
        }
        let serial = self
            .object_serial
            .checked_add(1)
            .filter(|&n| n < 0x0100_0000)
            .ok_or(8u32)?;
        let handle = 0x0f00_0000 | serial;
        self.object_serial = serial;
        self.objects.insert(handle, Object::Palette(entries));
        Ok(handle)
    }
    pub fn palette_entries(&self, handle: u32) -> std::result::Result<&[[u8; 4]], u32> {
        match self.objects.get(&handle) {
            Some(Object::Palette(entries)) => Ok(entries),
            _ => Err(6),
        }
    }
    pub fn select_palette(
        &mut self,
        dc: u32,
        palette: u32,
        force_background: bool,
    ) -> std::result::Result<u32, u32> {
        self.palette_entries(palette)?;
        let context = self.contexts.get_mut(&dc).ok_or(6u32)?;
        context.palette_background = force_background;
        Ok(std::mem::replace(&mut context.palette, palette))
    }
    pub fn realize_palette(&self, dc: u32, bits: u32) -> std::result::Result<u32, u32> {
        let context = self.contexts.get(&dc).ok_or(6u32)?;
        self.palette_entries(context.palette)?;
        if bits <= 8 {
            return Err(50);
        } // System-palette realization is not implemented for indexed displays.
        Ok(0)
    }
}
