//! DirectSound circular-buffer writes. Unlock updates the addressed frames; it does not enqueue audio.
use super::*;

#[derive(Debug, PartialEq, Eq)]
pub struct LockedRegions {
    pub first_address: u32,
    pub first_bytes: u32,
    pub second_address: u32,
    pub second_bytes: u32,
}

impl Audio {
    pub fn lock(&mut self, id: u32, offset: u32, length: u32, flags: u32) -> Result<LockedRegions> {
        let b = self.buffer(id)?;
        let start = if flags & 1 != 0 { b.write } else { offset };
        let length = if flags & 2 != 0 { b.bytes } else { length };
        if start >= b.bytes || length > b.bytes {
            return Err(Error::InvalidRange);
        }
        let first = length.min(b.bytes - start);
        let second = length - first;
        Ok(LockedRegions {
            first_address: b.address + start,
            first_bytes: first,
            second_address: if second != 0 { b.address } else { 0 },
            second_bytes: second,
        })
    }
    pub fn unlock<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        id: u32,
        first: (u32, u32),
        second: (u32, u32),
    ) -> Result<()> {
        let b = self.buffer(id)?;
        let (p1, mut n1) = first;
        let (p2, mut n2) = second;
        let inside = |p| p >= b.address && u64::from(p) < u64::from(b.address) + u64::from(b.bytes);
        if (p1 != 0 && !inside(p1)) || (p2 != 0 && !inside(p2)) {
            return Err(Error::InvalidRange);
        }
        if p1 != 0 {
            n1 = n1.min(b.bytes - (p1 - b.address));
        }
        if p2 != 0 {
            n2 = n2.min(b.bytes - (p2 - b.address));
        } else {
            n2 = 0;
        }
        // Snapshot both checked regions before mutating playback state.
        let (first_offset, first) = snapshot_frames(process, b, p1, n1)?;
        let (second_offset, second) = snapshot_frames(process, b, p2, n2)?;
        feed(b, first_offset, &first);
        feed(b, second_offset, &second);
        b.write =
            ((u64::from(b.write) + u64::from(n1) + u64::from(n2)) % u64::from(b.bytes)) as u32;
        Ok(())
    }
    pub fn duplicate_buffer<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
    ) -> Result<u32> {
        let source = self.buffer(id)?.clone();
        let mut copy = source.clone();
        copy.reset();
        let bytes = snapshot(process, source.address, source.bytes)?;
        copy.address = process.heap_alloc(source.bytes)?;
        process.write(copy.address, &bytes)?;
        let id = self.next.checked_add(1).ok_or(Error::OutOfMemory)?;
        self.next = id;
        self.buffers.insert(id, copy);
        Ok(id)
    }
}
fn snapshot<M: GuestMemory>(process: &Process<M>, pointer: u32, size: u32) -> Result<Vec<u8>> {
    if pointer == 0 {
        return Ok(Vec::new());
    }
    let mut bytes = vec![0; size as usize];
    process.read(pointer, &mut bytes)?;
    Ok(bytes)
}

// A lock can cover part of a sample. Refresh complete intersecting frames from
// guest memory so untouched bytes/channels remain intact in the PCM mirror.
fn snapshot_frames<M: GuestMemory>(
    process: &Process<M>,
    b: &Buffer,
    pointer: u32,
    size: u32,
) -> Result<(u32, Vec<u8>)> {
    if pointer == 0 || size == 0 {
        return Ok((0, Vec::new()));
    }
    let offset = pointer - b.address;
    let start = offset / b.format.align * b.format.align;
    let end = (offset + size).div_ceil(b.format.align) * b.format.align;
    Ok((start, snapshot(process, b.address + start, end - start)?))
}

fn feed(b: &mut Buffer, offset: u32, bytes: &[u8]) {
    let block = b.format.align;
    let frames = bytes.len() as u32 / block;
    if frames == 0 {
        return;
    }
    if b.pcm.is_empty() {
        b.pcm = vec![0.0; (b.bytes / block) as usize * b.format.channels as usize];
    }
    for frame in 0..frames {
        let index = (offset / block + frame) * b.format.channels;
        for channel in 0..b.format.channels {
            // Preserve the original decoder contract: 8-bit unsigned, otherwise 16-bit signed.
            let sample = if b.format.bits == 8 {
                (bytes[(frame * block + channel) as usize] as f32 - 128.0) / 128.0
            } else {
                let offset = (frame * block + channel * 2) as usize;
                i16::from_le_bytes([bytes[offset], bytes[offset + 1]]) as f32 / 32768.0
            };
            b.pcm[(index + channel) as usize] = sample;
        }
    }
}
