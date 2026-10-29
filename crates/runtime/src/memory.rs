//! Bulk guest-memory operations. Implementations must return errors, never expose raw pointers.
use crate::{Error, Result};
use std::collections::BTreeMap;

/// A little-endian DWORD update, returning the value before the write.
#[derive(Clone, Copy, Debug)]
pub enum U32Update {
    Add(u32),
    Exchange(u32),
    CompareExchange { exchange: u32, comparand: u32 },
}

impl U32Update {
    pub(crate) fn apply(self, previous: u32) -> u32 {
        match self {
            Self::Add(value) => previous.wrapping_add(value),
            Self::Exchange(value) => value,
            Self::CompareExchange {
                exchange,
                comparand,
            } => {
                if previous == comparand {
                    exchange
                } else {
                    previous
                }
            }
        }
    }
}

pub trait GuestMemory {
    fn ensure(&mut self, end: u64) -> Result<()>;
    fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()>;
    fn write(&mut self, address: u32, bytes: &[u8]) -> Result<()>;
    fn fill(&mut self, address: u32, size: u32, value: u8) -> Result<()>;
    fn copy(&mut self, destination: u32, source: u32, size: u32) -> Result<()>;

    /// This operation cannot suspend. It is atomic only within one execution worker.
    /// Always write, including a failed comparison, so backing write errors are retained.
    fn update_u32(&mut self, address: u32, operation: U32Update) -> Result<u32> {
        let mut bytes = [0; 4];
        self.read(address, &mut bytes)?;
        let previous = u32::from_le_bytes(bytes);
        self.write(address, &operation.apply(previous).to_le_bytes())?;
        Ok(previous)
    }
}

const PAGE: usize = 4096;

/// Sparse backing for native tooling/tests; zero pages need no physical allocation.
#[derive(Default)]
pub struct SparseMemory {
    length: u64,
    pages: BTreeMap<u32, Box<[u8; PAGE]>>,
}

impl SparseMemory {
    fn check(&self, address: u32, size: usize) -> Result<()> {
        if u64::from(address) + size as u64 > self.length {
            return Err(Error::Unmapped);
        }
        Ok(())
    }
}

impl GuestMemory for SparseMemory {
    fn ensure(&mut self, end: u64) -> Result<()> {
        if end > 1u64 << 32 {
            return Err(Error::OutOfMemory);
        }
        self.length = self.length.max(end);
        Ok(())
    }

    fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()> {
        self.check(address, bytes.len())?;
        for (offset, byte) in bytes.iter_mut().enumerate() {
            let address = u64::from(address) + offset as u64;
            *byte = self
                .pages
                .get(&((address / PAGE as u64) as u32))
                .map_or(0, |page| page[address as usize % PAGE]);
        }
        Ok(())
    }

    fn write(&mut self, address: u32, bytes: &[u8]) -> Result<()> {
        self.check(address, bytes.len())?;
        let mut offset = 0;
        while offset < bytes.len() {
            let address = u64::from(address) + offset as u64;
            let slot = address as usize % PAGE;
            let length = (PAGE - slot).min(bytes.len() - offset);
            let page = self
                .pages
                .entry((address / PAGE as u64) as u32)
                .or_insert_with(|| Box::new([0; PAGE]));
            page[slot..slot + length].copy_from_slice(&bytes[offset..offset + length]);
            offset += length;
        }
        Ok(())
    }

    fn fill(&mut self, address: u32, size: u32, value: u8) -> Result<()> {
        self.check(address, size as usize)?;
        let end = u64::from(address) + u64::from(size);
        let mut current = u64::from(address);
        while current < end {
            let index = (current / PAGE as u64) as u32;
            let slot = current as usize % PAGE;
            let length = ((PAGE - slot) as u64).min(end - current) as usize;
            if value != 0 || self.pages.contains_key(&index) {
                self.pages
                    .entry(index)
                    .or_insert_with(|| Box::new([0; PAGE]))[slot..slot + length]
                    .fill(value);
            }
            current += length as u64;
        }
        Ok(())
    }

    fn copy(&mut self, destination: u32, source: u32, size: u32) -> Result<()> {
        self.check(source, size as usize)?;
        self.check(destination, size as usize)?;
        // Bounded scratch space, with memmove ordering for overlapping regions.
        let mut scratch = [0; PAGE];
        let backwards = destination > source;
        let mut remaining = size;
        while remaining > 0 {
            let length = remaining.min(PAGE as u32);
            let offset = if backwards {
                remaining - length
            } else {
                size - remaining
            };
            self.read(source + offset, &mut scratch[..length as usize])?;
            self.write(destination + offset, &scratch[..length as usize])?;
            remaining -= length;
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "../tests/unit/memory.rs"]
mod tests;
