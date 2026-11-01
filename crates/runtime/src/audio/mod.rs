//! Process-owned DirectSound PCM state ported from runtime-rs/src/audio.
//! COM/vtable dispatch is separate; this module returns owned stereo blocks to the existing worklet transport.
mod buffer;
mod com;
mod controls;
mod device;
mod mixer;
pub(crate) mod music;
#[cfg(test)]
#[path = "../../tests/unit/audio/primary.rs"]
mod primary_tests;
#[cfg(test)]
#[path = "../../tests/unit/audio/mod.rs"]
mod tests;
mod transfer;
use crate::{Error, GuestMemory, Process, Result};
use buffer::Buffer;
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct PcmFormat {
    pub channels: u32,
    pub rate: u32,
    pub bits: u32,
    pub align: u32,
}

pub struct StereoBlock {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

#[derive(Default)]
pub struct Audio {
    adapter: com::Adapter,
    buffers: BTreeMap<u32, Buffer>,
    next: u32,
}

impl Audio {
    /// Allocate the requested frame-aligned circular buffer. Allocation padding must not change its playback length.
    pub fn create_buffer<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        size: u32,
        format: PcmFormat,
        flags: u32,
    ) -> Result<u32> {
        if format.channels == 0
            || format.rate == 0
            || format.bits < 8
            || !format.bits.is_multiple_of(8)
            || format.channels.checked_mul(format.bits / 8) != Some(format.align)
            || size < 4
        {
            return Err(Error::InvalidRange);
        }
        if flags & 0x10 != 0 {
            return Err(Error::Host("3D audio buffers are unsupported".into()));
        }
        let bytes = size
            .checked_add(format.align - 1)
            .ok_or(Error::InvalidRange)?
            / format.align
            * format.align;
        let address = process.heap_alloc(bytes)?;
        let id = self.next.checked_add(1).ok_or(Error::OutOfMemory)?;
        self.next = id;
        self.buffers.insert(
            id,
            Buffer {
                address,
                bytes,
                flags,
                primary: flags & 1 != 0,
                freq: format.rate,
                format,
                volume: 0,
                pan: 0,
                playing: false,
                looping: false,
                started: 0.0,
                base: 0,
                write: 0,
                pcm: Vec::new(),
                read: 0.0,
            },
        );
        Ok(id)
    }
    fn buffer(&mut self, id: u32) -> Result<&mut Buffer> {
        self.buffers.get_mut(&id).ok_or(Error::InvalidAllocation)
    }
    pub fn play(&mut self, id: u32, looping: bool, now_ms: f64) -> Result<()> {
        valid_time(now_ms)?;
        let b = self.buffer(id)?;
        b.playing = true;
        b.looping = looping;
        b.started = now_ms;
        Ok(())
    }
    pub fn stop(&mut self, id: u32) -> Result<()> {
        self.buffer(id)?.playing = false;
        Ok(())
    }
    pub fn seek(&mut self, id: u32, position: u32, now_ms: f64) -> Result<()> {
        valid_time(now_ms)?;
        let b = self.buffer(id)?;
        if position >= b.bytes {
            return Err(Error::InvalidRange);
        }
        let align = b.format.align.max(1);
        b.base = position - position % align;
        b.read = (b.base / align) as f64;
        b.started = now_ms;
        Ok(())
    }
    pub fn status(&mut self, id: u32, now_ms: f64) -> Result<(bool, u32, u32)> {
        valid_time(now_ms)?;
        let b = self.buffer(id)?;
        let position = b.position(now_ms);
        let playing = b.playing(now_ms);
        let write = if playing {
            (position + b.format.rate / 100 * b.format.align) % b.bytes
        } else {
            position
        };
        Ok((playing, position, write))
    }
    pub fn set_volume(&mut self, id: u32, volume: i32) -> Result<()> {
        let b = self.buffer(id)?;
        if b.flags & 128 == 0 || !(-10000..=0).contains(&volume) {
            return Err(Error::InvalidRange);
        }
        b.volume = volume;
        Ok(())
    }
    pub fn set_pan(&mut self, id: u32, pan: i32) -> Result<()> {
        let b = self.buffer(id)?;
        if b.flags & 64 == 0 || !(-10000..=10000).contains(&pan) {
            return Err(Error::InvalidRange);
        }
        b.pan = pan;
        Ok(())
    }
    pub fn set_frequency(&mut self, id: u32, rate: u32) -> Result<()> {
        let b = self.buffer(id)?;
        if b.primary || b.flags & 32 == 0 || (rate != 0 && !(100..=200000).contains(&rate)) {
            return Err(Error::InvalidRange);
        }
        b.freq = if rate == 0 { b.format.rate } else { rate };
        Ok(())
    }
    pub fn remove_buffer<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
    ) -> Result<()> {
        let address = self.buffer(id)?.address;
        process.heap_free(address)?;
        self.buffers.remove(&id);
        Ok(())
    }
    pub fn mix(&mut self, count: u32, rate: f64, now_ms: f64) -> Result<StereoBlock> {
        valid_time(now_ms)?;
        if count > 4096 || !(8000.0..=192000.0).contains(&rate) {
            return Err(Error::InvalidRange);
        }
        let mut block = StereoBlock {
            left: vec![0.0; count as usize],
            right: vec![0.0; count as usize],
        };
        for buffer in self.buffers.values_mut() {
            if buffer.playing(now_ms) && !buffer.pcm.is_empty() {
                mixer::mix(buffer, &mut block.left, &mut block.right, rate);
            }
        }
        Ok(block)
    }
}
fn valid_time(now_ms: f64) -> Result<()> {
    if !now_ms.is_finite() || now_ms < 0.0 {
        Err(Error::InvalidRange)
    } else {
        Ok(())
    }
}
