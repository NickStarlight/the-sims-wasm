//! Read-only MMIO snapshot/cursor behavior from the working runtime.
use super::*;
pub(super) struct File {
    bytes: Vec<u8>,
    position: i64,
}
impl Media {
    pub(crate) fn open(&mut self, bytes: Vec<u8>) -> Result<u32> {
        let id = self
            .files
            .keys()
            .next_back()
            .copied()
            .unwrap_or(0xfff)
            .checked_add(1)
            .ok_or(Error::OutOfMemory)?;
        self.files.insert(id, File { bytes, position: 0 });
        Ok(id)
    }
    pub(crate) fn mmio<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        match name {
            "mmioRead" => {
                let Some(file) = self.files.get_mut(&a[0]) else {
                    return Ok(0);
                };
                let start = file.position.clamp(0, file.bytes.len() as i64) as usize;
                let count = (a[2] as usize).min(file.bytes.len() - start);
                if count != 0 {
                    p.write(a[1], &file.bytes[start..start + count])?;
                }
                file.position += count as i64;
                Ok(count as u32)
            }
            "mmioSeek" => {
                let Some(file) = self.files.get_mut(&a[0]) else {
                    return Ok(u32::MAX);
                };
                let base = match a[2] {
                    0 => 0,
                    1 => file.position,
                    _ => file.bytes.len() as i64,
                };
                file.position = base + i64::from(a[1] as i32);
                Ok(
                    if file.position < 0 || file.position > file.bytes.len() as i64 {
                        u32::MAX
                    } else {
                        file.position as u32
                    },
                )
            }
            // The original host accepted caller buffer hints but retained its own byte snapshot.
            "mmioSetBuffer" => Ok(0),
            "mmioClose" => {
                self.files.remove(&a[0]);
                Ok(0)
            }
            _ => Err(Error::Host(format!("unsupported MMIO service {name}"))),
        }
    }
}
