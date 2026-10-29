//! DirectSound device slots ported from the original runtime; primary buffers reuse one object.
use super::com::*;
use super::*;
use crate::modules::ModuleCatalog;

// Primary storage is device-owned, not sized by DSBUFFERDESC.dwBufferBytes.
// Match Wine's software device capacity (ds_hel_buflen). Stream clients may
// size their secondary buffer from these caps; a tiny placeholder can leave
// them unable to enqueue even one decoded audio packet.
// https://github.com/wine-mirror/wine/blob/master/dlls/dsound/dsound_main.c
const PRIMARY_BUFFER_BYTES: u32 = 65536;
impl Audio {
    pub(super) fn device_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        slot: u32,
        a: &[u32],
    ) -> Result<u32> {
        match slot {
            1 | 2 => Ok(1), // Preserve original shared-device lifetime policy.
            3 => self.create_sound_buffer(p, modules, a),
            4 => {
                if a[1] == 0 {
                    return Ok(INVALID);
                }
                let size = get(p, a[1])?;
                if !(96..=65536).contains(&size) {
                    return Ok(INVALID);
                }
                p.write(a[1], &vec![0; size as usize])?;
                for (index, value) in [size, 0xf5f, 100, 200000, 1].into_iter().enumerate() {
                    put(p, a[1] + index as u32 * 4, value)?;
                }
                Ok(0)
            }
            5 => {
                if a[2] == 0 {
                    return Ok(INVALID);
                }
                put(p, a[2], 0)?;
                let Some(id) = self.buffer_id(a[1]) else {
                    return Ok(INVALID);
                };
                if self.buffer(id)?.primary {
                    return Ok(0x88780032);
                }
                let id = self.duplicate_buffer(p, id)?;
                let object = self.object(p, modules, Some(id))?;
                put(p, a[2], object)?;
                Ok(0)
            }
            6 => {
                self.adapter.priority = a[2];
                Ok(0)
            }
            7 => Ok(if self.adapter.priority < 2 {
                0x88780046
            } else {
                0
            }),
            8 | 11 => {
                if a[1] != 0 {
                    put(p, a[1], if slot == 8 { 0x20001 } else { 0 })?;
                }
                Ok(0)
            }
            9 => Ok(0),
            10 => Ok(INITIALIZED),
            _ => Ok(UNSUPPORTED),
        }
    }
    fn create_sound_buffer<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        a: &[u32],
    ) -> Result<u32> {
        if a[2] == 0 {
            return Ok(INVALID);
        }
        put(p, a[2], 0)?;
        if a[1] == 0 || ![20, 24].contains(&get(p, a[1])?) {
            return Ok(INVALID);
        }
        let flags = get(p, a[1] + 4)?;
        let size = get(p, a[1] + 8)?;
        let fmt = get(p, a[1] + 16)?;
        if flags & 16 != 0 {
            return Ok(UNSUPPORTED);
        }
        let primary = flags & 1 != 0;
        let format = if primary {
            if fmt != 0 {
                return Ok(INVALID);
            }
            PcmFormat {
                channels: 2,
                rate: 22050,
                bits: 16,
                align: 4,
            }
        } else {
            if fmt == 0 {
                return Ok(INVALID);
            }
            let (tag, format) = read_format(p, fmt)?;
            if ![1, 0xfffe].contains(&tag) {
                return Ok(UNSUPPORTED);
            }
            if !valid_format(p, fmt, &format)? || size < 4 {
                return Ok(INVALID);
            }
            format
        };
        if primary && self.adapter.primary != 0 {
            put(p, a[2], self.adapter.primary)?;
            return Ok(0);
        }
        let id = match self.create_buffer(
            p,
            if primary { PRIMARY_BUFFER_BYTES } else { size },
            format,
            flags,
        ) {
            Ok(id) => id,
            Err(Error::InvalidRange) => return Ok(INVALID),
            Err(Error::OutOfMemory) => return Ok(0x8007000e),
            Err(e) => return Err(e),
        };
        let object = self.object(p, modules, Some(id))?;
        if primary {
            self.adapter.primary = object;
        }
        put(p, a[2], object)?;
        Ok(0)
    }
}
pub(super) fn read_format<M: GuestMemory>(
    p: &Process<M>,
    address: u32,
) -> Result<(u16, PcmFormat)> {
    let mut b = [0; 16];
    p.read(address, &mut b)?;
    Ok((
        u16::from_le_bytes([b[0], b[1]]),
        PcmFormat {
            channels: u16::from_le_bytes([b[2], b[3]]).into(),
            rate: u32::from_le_bytes(b[4..8].try_into().unwrap()),
            align: u16::from_le_bytes([b[12], b[13]]).into(),
            bits: u16::from_le_bytes([b[14], b[15]]).into(),
        },
    ))
}
pub(super) fn valid_format<M: GuestMemory>(
    p: &Process<M>,
    address: u32,
    f: &PcmFormat,
) -> Result<bool> {
    Ok(f.bits >= 8
        && f.bits.is_multiple_of(8)
        && f.channels != 0
        && f.rate != 0
        && get(p, address + 8)? != 0
        && f.channels.checked_mul(f.bits / 8) == Some(f.align))
}
