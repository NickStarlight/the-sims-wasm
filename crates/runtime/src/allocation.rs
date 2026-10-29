//! Heap ownership and reusable blocks live outside the guest address space.
use crate::{regions::Regions, Error, GuestMemory, Region, RegionKind, Result};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy)]
struct Allocation {
    capacity: u32,
    requested: u32,
}

pub(crate) struct Heap {
    cursor: u64,
    free: BTreeMap<u32, u32>,
    free_by_size: BTreeSet<(u32, u32)>,
    live: BTreeMap<u32, Allocation>,
}

impl Heap {
    pub fn new(start: u32) -> Self {
        Self {
            cursor: u64::from(start),
            free: BTreeMap::new(),
            free_by_size: BTreeSet::new(),
            live: BTreeMap::new(),
        }
    }

    pub fn allocate(
        &mut self,
        size: u32,
        memory: &mut impl GuestMemory,
        regions: &mut Regions,
        limit: u64,
    ) -> Result<u32> {
        let capacity = capacity(size)?;
        let reusable = self
            .free_by_size
            .range((capacity, 0)..)
            .next()
            .map(|&(size, address)| (address, size));
        let address = if let Some((address, _)) = reusable {
            address
        } else {
            regions.available(self.cursor, capacity, 16, limit)?
        };
        memory.ensure(u64::from(address) + u64::from(capacity))?;
        memory.fill(address, capacity, 0)?;
        if let Some((address, available)) = reusable {
            self.remove_free(address);
            if available > capacity {
                self.insert_free(address + capacity, available - capacity);
            }
        } else {
            regions.insert(Region {
                address,
                size: capacity,
                kind: RegionKind::Heap,
            });
            self.cursor = u64::from(address) + u64::from(capacity);
        }
        self.live.insert(
            address,
            Allocation {
                capacity,
                requested: size,
            },
        );
        Ok(address)
    }

    /// Resize without moving guest bytes. Ownership changes only after memory operations succeed.
    pub fn resize(
        &mut self,
        address: u32,
        size: u32,
        memory: &mut impl GuestMemory,
        regions: &mut Regions,
        limit: u64,
    ) -> Result<bool> {
        let old = *self.live.get(&address).ok_or(Error::InvalidAllocation)?;
        let capacity = capacity(size)?;
        let end = u64::from(address) + u64::from(old.capacity);
        let growth = capacity.saturating_sub(old.capacity);
        let adjacent = u32::try_from(end)
            .ok()
            .and_then(|end| self.free.get(&end).copied())
            .filter(|&available| available >= growth);
        if growth != 0 && adjacent.is_none() {
            // Extend only the frontier, without crossing an image, stack, or VirtualAlloc reservation.
            if end != self.cursor
                || u32::try_from(end)
                    .ok()
                    .is_none_or(|end| regions.vacant(end, growth, limit).is_err())
            {
                return Ok(false);
            }
            memory.ensure(end + u64::from(growth))?;
        }
        if size > old.requested {
            memory.fill(address + old.requested, size - old.requested, 0)?;
        }
        if growth != 0 {
            let end = end as u32;
            if let Some(available) = adjacent {
                self.remove_free(end);
                if available > growth {
                    self.insert_free(end + growth, available - growth);
                }
            } else {
                regions.insert(Region {
                    address: end,
                    size: growth,
                    kind: RegionKind::Heap,
                });
                self.cursor += u64::from(growth);
            }
        } else if capacity < old.capacity {
            self.release(address + capacity, old.capacity - capacity);
        }
        self.live.insert(
            address,
            Allocation {
                capacity,
                requested: size,
            },
        );
        Ok(true)
    }

    pub fn size(&self, address: u32) -> Result<u32> {
        self.live
            .get(&address)
            .map(|record| record.requested)
            .ok_or(Error::InvalidAllocation)
    }

    pub fn free(&mut self, address: u32) -> Result<()> {
        if address == 0 {
            return Ok(());
        }
        let allocation = self.live.remove(&address).ok_or(Error::InvalidAllocation)?;
        self.release(address, allocation.capacity);
        Ok(())
    }

    fn insert_free(&mut self, address: u32, size: u32) {
        self.free.insert(address, size);
        self.free_by_size.insert((size, address));
    }

    fn remove_free(&mut self, address: u32) {
        let size = self.free.remove(&address).unwrap();
        self.free_by_size.remove(&(size, address));
    }

    fn release(&mut self, address: u32, mut size: u32) {
        let mut start = address;
        if let Some((&previous, &length)) = self.free.range(..address).next_back() {
            if u64::from(previous) + u64::from(length) == u64::from(address) {
                self.remove_free(previous);
                start = previous;
                size += length;
            }
        }
        if let Some((&next, &length)) = self.free.range(address..).next() {
            if u64::from(start) + u64::from(size) == u64::from(next) {
                self.remove_free(next);
                size += length;
            }
        }
        self.insert_free(start, size);
    }
}

fn capacity(size: u32) -> Result<u32> {
    Ok(size.max(1).checked_add(15).ok_or(Error::OutOfMemory)? & !15)
}

#[cfg(test)]
#[path = "../tests/unit/allocation.rs"]
mod tests;
