//! Reserve/commit state follows VirtualAlloc's page ownership contract.
//! https://learn.microsoft.com/windows/win32/api/memoryapi/nf-memoryapi-virtualalloc
use super::Process;
use crate::{Error, GuestMemory, Region, RegionKind, Result};
use std::collections::{BTreeMap, BTreeSet};

const PAGE: u64 = 4096;
const GRANULARITY: u64 = 65536;
const COMMIT: u32 = 0x1000;
const RESERVE: u32 = 0x2000;

struct Reservation {
    end: u64,
    committed: BTreeSet<u32>,
}

#[derive(Default)]
pub(super) struct VirtualMemory(BTreeMap<u32, Reservation>);

impl<M: GuestMemory> Process<M> {
    /// Convenience allocation: reserve and commit writable pages in one operation.
    pub fn virtual_alloc(&mut self, address: u32, size: u32) -> Result<u32> {
        self.virtual_alloc_flags(address, size, RESERVE | COMMIT, 4)
    }

    /// Reserve address space or commit pages within an existing owned reservation.
    /// Recommitting pages preserves their contents; only newly committed pages are zeroed.
    pub fn virtual_alloc_flags(
        &mut self,
        address: u32,
        size: u32,
        kind: u32,
        protect: u32,
    ) -> Result<u32> {
        // A reservation has no accessible pages. Complete reserves its arena
        // with PAGE_NOACCESS, then commits smaller PAGE_READWRITE ranges.
        let supported_protection = protect == 4 || protect == 1 && kind == RESERVE;
        if !matches!(kind, COMMIT | RESERVE | 0x3000) || !supported_protection {
            return Err(Error::Host(
                "unsupported VirtualAlloc allocation/protection mode".into(),
            ));
        }
        if size == 0 {
            return Err(Error::InvalidRange);
        }
        let reserve = kind & RESERVE != 0 || address == 0;
        let alignment = if reserve { GRANULARITY } else { PAGE };
        let start = if address == 0 {
            let length = u64::from(size).div_ceil(PAGE) * PAGE;
            self.regions.available(
                u64::from(self.allocation_start),
                u32::try_from(length).map_err(|_| Error::OutOfMemory)?,
                GRANULARITY,
                self.limit,
            )?
        } else {
            (u64::from(address) / alignment * alignment) as u32
        };
        let end = (u64::from(if address == 0 { start } else { address }) + u64::from(size))
            .div_ceil(PAGE)
            * PAGE;
        if start == 0 || end > self.limit {
            return Err(Error::InvalidRange);
        }
        let length = u32::try_from(end - u64::from(start)).map_err(|_| Error::OutOfMemory)?;
        let owner = if reserve {
            self.regions.vacant(start, length, self.limit)?;
            start
        } else {
            self.virtual_memory
                .0
                .range(..=start)
                .next_back()
                .filter(|(_, reservation)| reservation.end >= end)
                .map(|(&base, _)| base)
                .ok_or(Error::Overlap)?
        };
        if kind & COMMIT != 0 {
            self.commit_pages(owner, start, end, reserve)?;
        }
        if reserve {
            let committed = if kind & COMMIT != 0 {
                pages(start, end).collect()
            } else {
                BTreeSet::new()
            };
            self.virtual_memory
                .0
                .insert(start, Reservation { end, committed });
            self.regions.insert(Region {
                address: start,
                size: length,
                kind: RegionKind::Virtual,
            });
        }
        Ok(start)
    }

    fn commit_pages(&mut self, owner: u32, start: u32, end: u64, new: bool) -> Result<()> {
        let missing: Vec<_> = pages(start, end)
            .filter(|page| new || !self.virtual_memory.0[&owner].committed.contains(page))
            .collect();
        if missing.is_empty() {
            return Ok(());
        }
        self.memory.ensure(end)?;
        for (start, size) in contiguous_runs(&missing) {
            self.memory.fill(start, size, 0)?;
        }
        if !new {
            self.virtual_memory
                .0
                .get_mut(&owner)
                .unwrap()
                .committed
                .extend(missing);
        }
        Ok(())
    }

    /// Decommit pages while retaining their reservation for the CRT's later reuse.
    pub fn virtual_decommit(&mut self, address: u32, size: u32) -> Result<()> {
        let (&owner, reservation) = self
            .virtual_memory
            .0
            .range(..=address)
            .next_back()
            .filter(|(_, r)| u64::from(address) < r.end)
            .ok_or(Error::InvalidAllocation)?;
        let (start, end) = if size == 0 {
            if address != owner {
                return Err(Error::InvalidRange);
            }
            (owner, reservation.end)
        } else {
            (
                (u64::from(address) / PAGE * PAGE) as u32,
                (u64::from(address) + u64::from(size)).div_ceil(PAGE) * PAGE,
            )
        };
        if end > reservation.end {
            return Err(Error::InvalidRange);
        }
        let committed: Vec<_> = pages(start, end)
            .filter(|page| reservation.committed.contains(page))
            .collect();
        for (start, size) in contiguous_runs(&committed) {
            self.memory.fill(start, size, 0)?;
        }
        let reservation = self.virtual_memory.0.get_mut(&owner).unwrap();
        for page in committed {
            reservation.committed.remove(&page);
        }
        Ok(())
    }

    pub fn virtual_free(&mut self, address: u32) -> Result<()> {
        let reservation = self
            .virtual_memory
            .0
            .get(&address)
            .ok_or(Error::InvalidAllocation)?;
        let committed: Vec<_> = reservation.committed.iter().copied().collect();
        for (start, size) in contiguous_runs(&committed) {
            self.memory.fill(start, size, 0)?;
        }
        self.virtual_memory.0.remove(&address);
        self.regions.remove(address);
        Ok(())
    }

    pub(super) fn check_virtual_access(&self, address: u32, size: u32) -> Result<()> {
        if size == 0 {
            return Ok(());
        }
        let end = u64::from(address) + u64::from(size);
        for (&base, reservation) in self
            .virtual_memory
            .0
            .range(..=address.saturating_add(size - 1))
        {
            let start = u64::from(address.max(base));
            let overlap_end = end.min(reservation.end);
            if start < overlap_end
                && pages(
                    (start / PAGE * PAGE) as u32,
                    overlap_end.div_ceil(PAGE) * PAGE,
                )
                .any(|page| !reservation.committed.contains(&page))
            {
                return Err(Error::Unmapped);
            }
        }
        Ok(())
    }
}

fn pages(start: u32, end: u64) -> impl Iterator<Item = u32> {
    (u64::from(start)..end)
        .step_by(PAGE as usize)
        .map(|page| page as u32)
}

/// Coalesce zeroing so large commits use bulk memory operations, not one host call per page.
fn contiguous_runs(pages: &[u32]) -> Vec<(u32, u32)> {
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for &page in pages {
        if let Some((start, size)) = runs.last_mut() {
            if u64::from(*start) + u64::from(*size) == u64::from(page) {
                *size += PAGE as u32;
                continue;
            }
        }
        runs.push((page, PAGE as u32));
    }
    runs
}
