//! Process address-space ownership, independent of guest-writable bytes.
use crate::{Error, Result};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionKind {
    Protected,
    ReadOnly,
    ReadWrite,
    Heap,
    Virtual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Region {
    pub address: u32,
    pub size: u32,
    pub kind: RegionKind,
}

impl Region {
    pub fn end(self) -> u64 {
        u64::from(self.address) + u64::from(self.size)
    }
}

#[derive(Default)]
pub(crate) struct Regions(BTreeMap<u32, Region>);

impl Regions {
    pub fn values(&self) -> impl Iterator<Item = Region> + '_ {
        self.0.values().copied()
    }

    pub fn vacant(&self, address: u32, size: u32, limit: u64) -> Result<()> {
        let end = checked_end(address, size, limit)?;
        if self
            .0
            .range(..=address)
            .next_back()
            .is_some_and(|(_, region)| region.end() > u64::from(address))
            || self
                .0
                .range(address..)
                .next()
                .is_some_and(|(&start, _)| u64::from(start) < end)
        {
            return Err(Error::Overlap);
        }
        Ok(())
    }

    pub fn insert(&mut self, region: Region) {
        self.0.insert(region.address, region);
    }
    pub fn remove(&mut self, address: u32) {
        self.0.remove(&address);
    }

    pub fn available(&self, from: u64, size: u32, alignment: u64, limit: u64) -> Result<u32> {
        let mut start = from.div_ceil(alignment) * alignment;
        loop {
            if start + u64::from(size) > limit || start > u64::from(u32::MAX) {
                return Err(Error::OutOfMemory);
            }
            let address = start as u32;
            let previous = self
                .0
                .range(..=address)
                .next_back()
                .map(|(_, value)| *value);
            let next = self.0.range(address..).next().map(|(_, value)| *value);
            let conflict = previous.filter(|region| region.end() > start).or_else(|| {
                next.filter(|region| u64::from(region.address) < start + u64::from(size))
            });
            match conflict {
                Some(region) => start = region.end().div_ceil(alignment) * alignment,
                None => return Ok(address),
            }
        }
    }

    pub fn access(&self, address: u32, size: u32, write: bool, limit: u64) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let end = checked_end(address, size, limit)?;
        let mut current = u64::from(address);
        while current < end {
            let region = self
                .0
                .range(..=current as u32)
                .next_back()
                .map(|(_, value)| value)
                .ok_or(Error::Unmapped)?;
            if region.end() <= current {
                return Err(Error::Unmapped);
            }
            if region.kind == RegionKind::Protected
                || (write && region.kind == RegionKind::ReadOnly)
            {
                return Err(Error::Protected);
            }
            current = region.end().min(end);
        }
        Ok(())
    }
}

pub(crate) fn checked_end(address: u32, size: u32, limit: u64) -> Result<u64> {
    let end = u64::from(address) + u64::from(size);
    if size == 0 || end > limit || limit > 1u64 << 32 {
        return Err(Error::InvalidRange);
    }
    Ok(end)
}
