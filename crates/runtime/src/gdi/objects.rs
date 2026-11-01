//! Stock object identities and Win32 logical metadata; no font rasterization claims.
use super::Gdi;
#[derive(Clone)]
pub(super) enum Object {
    Bitmap(super::bitmap::Bitmap),
    Font,
    Palette(Vec<[u8; 4]>),
    Brush { style: u32, color: u32 },
    Pen { style: u32, color: u32 },
}
impl Object {
    fn stock(index: u32) -> Option<Self> {
        match index {
            0..=4 => Some(Self::Brush {
                style: 0,
                color: [0xffffff, 0xc0c0c0, 0x808080, 0x404040, 0][index as usize],
            }),
            5 => Some(Self::Brush { style: 1, color: 0 }), // BS_NULL, not a solid black brush.
            6..=8 => Some(Self::Pen {
                style: if index == 8 { 5 } else { 0 },
                color: if index == 6 { 0xffffff } else { 0 },
            }),
            15 => Some(Self::Palette(super::palette::default_entries())),
            _ => None,
        }
    }
    fn record(self) -> Vec<u8> {
        let words = match self {
            Self::Bitmap(bitmap) => return bitmap.record(),
            Self::Font => return Vec::new(),
            Self::Palette(entries) => return (entries.len() as u16).to_le_bytes().to_vec(),
            Self::Brush { style, color } => vec![style, color, 0],
            Self::Pen { style, color } => vec![style, 1, 0, color],
        };
        words.into_iter().flat_map(u32::to_le_bytes).collect()
    }
}
impl Gdi {
    pub fn stock_object(&mut self, index: u32) -> std::result::Result<u32, u32> {
        let object = Object::stock(index).ok_or(if matches!(index, 10..=19) {
            50u32
        } else {
            87u32
        })?;
        let handle = 0x0e00_0000 | index;
        self.objects.entry(handle).or_insert(object);
        Ok(handle)
    }
    pub fn object_record(&self, handle: u32) -> std::result::Result<Vec<u8>, u32> {
        self.objects
            .get(&handle)
            .cloned()
            .map(Object::record)
            .ok_or(6)
    }
    /// Stock objects have process lifetime. DeleteObject succeeds without invalidating them.
    pub fn delete_object(&mut self, handle: u32) -> bool {
        if !self.objects.contains_key(&handle) {
            return false;
        }
        if handle & 0xff00_0000 == 0x0e00_0000 {
            return true;
        }
        if self
            .contexts
            .values()
            .any(|dc| dc.palette == handle || dc.selected == handle)
        {
            return false;
        }
        self.objects.remove(&handle);
        true
    }
}
