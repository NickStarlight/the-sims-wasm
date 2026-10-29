//! Port of legacy graphics/ddraw/clipper.rs with owned region snapshots.
use super::*;
#[derive(Default)]
pub(super) struct Clipper {
    window: u32,
    list: Option<Vec<u8>>,
    initialized: bool,
}
impl Graphics {
    pub(super) fn clipper_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let Some(c) = self.clippers.get_mut(&a[0]) else {
            return Ok(INVALID);
        };
        match name {
            "Clipper_GetClipList" => {
                let Some(list) = &c.list else {
                    return Ok(0x887600a0);
                };
                if a[3] != 0 {
                    write_u32(p, a[3], list.len() as u32)?;
                }
                if a[2] != 0 {
                    p.write(a[2], list)?;
                }
                Ok(0)
            }
            "Clipper_SetClipList" => {
                if c.window != 0 {
                    return Ok(0x88760088);
                }
                if a[1] == 0 {
                    c.list = None;
                } else {
                    let mut header = [0; 32];
                    p.read(a[1], &mut header)?;
                    let size = u32::from_le_bytes(header[12..16].try_into().unwrap())
                        .checked_add(32)
                        .ok_or(Error::InvalidRange)?;
                    let mut list = zero_bytes(size as usize)?;
                    p.read(a[1], &mut list)?;
                    c.list = Some(list);
                }
                Ok(0)
            }
            "Clipper_Initialize" => {
                if c.initialized {
                    return Ok(0x88760005);
                }
                c.initialized = true;
                Ok(0)
            }
            "Clipper_SetHWnd" => {
                if a[1] != 0 {
                    return Ok(INVALID);
                }
                c.window = a[2];
                Ok(0)
            }
            "Clipper_GetHWnd" => {
                if a[1] != 0 {
                    write_u32(p, a[1], c.window)?;
                }
                Ok(0)
            }
            "Clipper_IsClipListChanged" => {
                if a[1] != 0 {
                    write_u32(p, a[1], 0)?;
                }
                Ok(0)
            }
            _ => Ok(INVALID),
        }
    }
}
