//! Shared UI image handles retain real PE icon resources; standard cursors retain their system identity.
use super::*;
use resources::{resource, Name};

#[derive(Default)]
pub(super) struct Images {
    entries: BTreeMap<(u32, Name, bool), (u32, Asset)>,
}
enum Asset {
    Icon {
        // Retained unchanged for future decoding/drawing, never guest allocator metadata.
        _group: Vec<u8>,
        _images: Vec<Vec<u8>>,
    },
    StandardCursor,
}

impl Kernel {
    pub(super) fn load_image<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let cursor = call == Call::LoadCursorA;
        let name = if args[1] <= 0xffff {
            Name::Id(args[1] as u16)
        } else {
            let bytes = process.read_c_string(args[1], 32768)?;
            Name::Text(
                bytes
                    .into_iter()
                    .map(|byte| char::from_u32(u32::from(text::decode_byte(byte))).unwrap())
                    .collect(),
            )
        };
        let key = (args[0], name, cursor);
        if let Some((handle, _)) = self.images.entries.get(&key) {
            return Ok(*handle);
        }
        let asset = if cursor {
            if args[0] != 0 {
                return Err(Error::Host(
                    "application cursor resource loading is unsupported".into(),
                ));
            }
            if !matches!(
                key.1,
                Name::Id(32512..=32516 | 32640..=32646 | 32648..=32651)
            ) {
                return Ok(self.fail(1814));
            }
            Asset::StandardCursor
        } else {
            if args[0] == 0 {
                return Err(Error::Host(
                    "standard system icon assets are unsupported".into(),
                ));
            }
            let Some(path) = self.modules.filename(args[0]) else {
                return Ok(self.fail(6));
            };
            let loaded = match self.files.file_bytes(path)? {
                Some(bytes) => load_icon(&bytes, &key.1),
                None => Err(2),
            };
            match loaded {
                Ok(asset) => asset,
                Err(error) => return Ok(self.fail(error)),
            }
        };
        let handle = 0x7100_0000u32
            .checked_add(self.images.entries.len() as u32)
            .ok_or(Error::OutOfMemory)?;
        self.images.entries.insert(key, (handle, asset));
        Ok(handle)
    }
}

fn load_icon(bytes: &[u8], name: &Name) -> std::result::Result<Asset, u32> {
    let group = resource(bytes, 14, name)?;
    if group.len() < 6 || group[..4] != [0, 0, 1, 0] {
        return Err(13);
    }
    let count = u16::from_le_bytes([group[4], group[5]]) as usize;
    if count == 0 || count > 256 || group.len() != 6 + count * 14 {
        return Err(13);
    }
    let mut images = Vec::with_capacity(count);
    for entry in group[6..].as_chunks::<14>().0 {
        let size = u32::from_le_bytes(entry[8..12].try_into().unwrap()) as usize;
        let id = u16::from_le_bytes(entry[12..14].try_into().unwrap());
        let image = resource(bytes, 3, &Name::Id(id))?;
        if size == 0 || image.len() != size {
            return Err(13);
        }
        images.push(image.to_vec());
    }
    Ok(Asset::Icon {
        _group: group.to_vec(),
        _images: images,
    })
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/images.rs"]
mod tests;
