//! DirectSound buffer slots preserve the original HRESULT and control behavior.
use super::com::*;
use super::*;
impl Audio {
    pub(super) fn buffer_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        id: u32,
        slot: u32,
        a: &[u32],
        now: f64,
    ) -> Result<u32> {
        match slot {
            1 => Ok(1),
            2 => {
                self.stop(id)?;
                Ok(1)
            }
            3 => {
                if a[1] == 0 || get(p, a[1])? < 20 {
                    return Ok(INVALID);
                }
                let b = self.buffer(id)?;
                for (offset, value) in [(4, b.flags | 8), (8, b.bytes), (12, 0), (16, 0)] {
                    put(p, a[1] + offset, value)?;
                }
                Ok(0)
            }
            4 => {
                let (_, position, write) = self.status(id, now)?;
                if a[1] != 0 {
                    put(p, a[1], position)?;
                }
                if a[2] != 0 {
                    put(p, a[2], write)?;
                }
                Ok(0)
            }
            5 => self.get_format(p, id, a),
            6..=8 => {
                if a[1] == 0 {
                    return Ok(INVALID);
                }
                let b = self.buffer(id)?;
                let (mask, value) = match slot {
                    6 => (128, b.volume as u32),
                    7 => (64, b.pan as u32),
                    _ => (32, b.freq),
                };
                if b.flags & mask == 0 {
                    return Ok(CONTROL);
                }
                put(p, a[1], value)?;
                Ok(0)
            }
            9 => {
                if a[1] == 0 {
                    return Ok(INVALID);
                }
                let b = self.buffer(id)?;
                put(
                    p,
                    a[1],
                    if b.playing(now) {
                        1 | if b.looping { 4 } else { 0 }
                    } else {
                        0
                    },
                )?;
                Ok(0)
            }
            10 => Ok(INITIALIZED),
            11 => {
                if a[3] == 0 || a[4] == 0 {
                    return Ok(INVALID);
                }
                for ptr in &a[3..7] {
                    if *ptr != 0 {
                        put(p, *ptr, 0)?;
                    }
                }
                let lock = match self.lock(id, a[1], a[2], a[7]) {
                    Ok(lock) => lock,
                    Err(Error::InvalidRange) => return Ok(INVALID),
                    Err(e) => return Err(e),
                };
                for (ptr, value) in [
                    (a[3], lock.first_address),
                    (a[4], lock.first_bytes),
                    (a[5], lock.second_address),
                    (a[6], lock.second_bytes),
                ] {
                    if ptr != 0 {
                        put(p, ptr, value)?;
                    }
                }
                Ok(0)
            }
            12 => {
                if self.buffer(id)?.primary && a[3] & 1 == 0 {
                    return Ok(INVALID);
                }
                self.play(id, a[3] & 1 != 0, now)?;
                Ok(0)
            }
            13 => match self.seek(id, a[1], now) {
                Ok(()) => Ok(0),
                Err(Error::InvalidRange) => Ok(INVALID),
                Err(e) => Err(e),
            },
            14 => {
                if !self.buffer(id)?.primary {
                    return Ok(0x88780032);
                }
                if self.adapter.priority == 1 {
                    return Ok(0x88780046);
                }
                if a[1] == 0 {
                    return Ok(INVALID);
                }
                let (_, format) = device::read_format(p, a[1])?;
                if !device::valid_format(p, a[1], &format)? {
                    return Ok(INVALID);
                }
                let b = self.buffer(id)?;
                b.freq = format.rate;
                b.format = format;
                Ok(0)
            }
            15..=17 => self.control(id, slot, a[1]),
            18 => {
                self.stop(id)?;
                Ok(0)
            }
            19 => match self.unlock(p, id, (a[1], a[2]), (a[3], a[4])) {
                Ok(()) => Ok(0),
                Err(Error::InvalidRange) => Ok(INVALID),
                Err(e) => Err(e),
            },
            20 | 22 => Ok(0),
            _ => Ok(UNSUPPORTED),
        }
    }
    fn control(&mut self, id: u32, slot: u32, value: u32) -> Result<u32> {
        let b = self.buffer(id)?;
        let mask = match slot {
            15 => 128,
            16 => 64,
            _ => 32,
        };
        if slot == 16 && !(-10000..=10000).contains(&(value as i32)) {
            return Ok(INVALID);
        }
        if b.flags & mask == 0 || (slot == 17 && b.primary) {
            return Ok(CONTROL);
        }
        let result = match slot {
            15 => self.set_volume(id, value as i32),
            16 => self.set_pan(id, value as i32),
            _ => self.set_frequency(id, value),
        };
        match result {
            Ok(()) => Ok(0),
            Err(Error::InvalidRange) => Ok(INVALID),
            Err(e) => Err(e),
        }
    }
    fn get_format<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        id: u32,
        a: &[u32],
    ) -> Result<u32> {
        if a[1] == 0 {
            if a[3] == 0 {
                return Ok(INVALID);
            }
            put(p, a[3], 18)?;
            return Ok(0);
        }
        if a[2] < 18 {
            if a[3] != 0 {
                put(p, a[3], 0)?;
            }
            return Ok(INVALID);
        }
        let b = self.buffer(id)?;
        let mut bytes = Vec::with_capacity(18);
        bytes.extend(1u16.to_le_bytes());
        bytes.extend((b.format.channels as u16).to_le_bytes());
        bytes.extend(b.format.rate.to_le_bytes());
        bytes.extend(b.average().to_le_bytes());
        bytes.extend(((b.format.channels * (b.format.bits >> 3)) as u16).to_le_bytes());
        bytes.extend((b.format.bits as u16).to_le_bytes());
        bytes.extend(0u16.to_le_bytes());
        p.write(a[1], &bytes)?;
        if a[3] != 0 {
            put(p, a[3], 18)?;
        }
        Ok(0)
    }
}
