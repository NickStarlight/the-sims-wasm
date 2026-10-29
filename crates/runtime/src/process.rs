//! Explicit process lifetime and validated kernel memory operations.
use crate::{allocation::Heap, regions::Regions, Error, GuestMemory, Region, RegionKind, Result};

mod virtual_memory;
use virtual_memory::VirtualMemory;

pub struct Process<M> {
    memory: M,
    regions: Regions,
    heap: Heap,
    virtual_memory: VirtualMemory,
    limit: u64,
    allocation_start: u32,
}

impl<M: GuestMemory> Process<M> {
    pub fn new(memory: M, allocation_start: u32, limit: u64) -> Result<Self> {
        if allocation_start == 0 || u64::from(allocation_start) >= limit || limit > 1u64 << 32 {
            return Err(Error::InvalidRange);
        }
        Ok(Self {
            memory,
            regions: Regions::default(),
            heap: Heap::new(allocation_start),
            virtual_memory: VirtualMemory::default(),
            limit,
            allocation_start,
        })
    }

    /// Protect a loader-owned range against allocation and shim writes; no memory grows.
    pub fn reserve(&mut self, address: u32, size: u32) -> Result<()> {
        self.regions.vacant(address, size, self.limit)?;
        self.regions.insert(Region {
            address,
            size,
            kind: RegionKind::Protected,
        });
        Ok(())
    }

    /// Map image/stack bytes atomically with respect to process ownership metadata.
    pub fn map(&mut self, address: u32, bytes: &[u8], writable: bool) -> Result<()> {
        let size = u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?;
        self.regions.vacant(address, size, self.limit)?;
        self.memory.ensure(u64::from(address) + u64::from(size))?;
        self.memory.write(address, bytes)?;
        self.regions.insert(Region {
            address,
            size,
            kind: if writable {
                RegionKind::ReadWrite
            } else {
                RegionKind::ReadOnly
            },
        });
        Ok(())
    }

    /// Adopt bytes already loaded by the browser without copying them through Rust.
    pub fn adopt(&mut self, address: u32, size: u32, writable: bool) -> Result<()> {
        self.regions.vacant(address, size, self.limit)?;
        self.memory.ensure(u64::from(address) + u64::from(size))?;
        self.regions.insert(Region {
            address,
            size,
            kind: if writable {
                RegionKind::ReadWrite
            } else {
                RegionKind::ReadOnly
            },
        });
        Ok(())
    }

    pub fn regions(&self) -> impl Iterator<Item = Region> + '_ {
        self.regions.values()
    }

    pub(crate) fn address_limit(&self) -> u64 {
        self.limit
    }

    pub fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()> {
        self.regions.access(
            address,
            u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
            false,
            self.limit,
        )?;
        self.check_virtual_access(address, bytes.len() as u32)?;
        self.memory.read(address, bytes)
    }

    pub fn write(&mut self, address: u32, bytes: &[u8]) -> Result<()> {
        self.regions.access(
            address,
            u32::try_from(bytes.len()).map_err(|_| Error::InvalidRange)?,
            true,
            self.limit,
        )?;
        self.check_virtual_access(address, bytes.len() as u32)?;
        self.memory.write(address, bytes)
    }

    /// Validate readable/writable ownership once, even when the value stays unchanged.
    /// Writable regions are also readable; virtual reservations must be committed.
    pub fn update_u32(&mut self, address: u32, operation: crate::U32Update) -> Result<u32> {
        if let Err(error) = self.regions.access(address, 4, true, self.limit) {
            // Keep read-before-write error precedence on invalid spans: a DWORD
            // can begin in read-only bytes and end in an unmapped gap. Valid
            // updates still perform just one ownership check.
            self.read(address, &mut [0; 4])?;
            return Err(error);
        }
        self.check_virtual_access(address, 4)?;
        self.memory.update_u32(address, operation)
    }

    /// Read a bounded NUL-terminated byte string using bulk transfers where mappings permit.
    pub fn read_c_string(&self, address: u32, maximum: u32) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        while bytes.len() < maximum as usize {
            let current = address
                .checked_add(bytes.len() as u32)
                .ok_or(Error::InvalidRange)?;
            let mut block = [0; 128];
            let requested = block.len().min(maximum as usize - bytes.len());
            let length = if self.read(current, &mut block[..requested]).is_ok() {
                requested
            } else {
                self.read(current, &mut block[..1])?;
                1
            };
            if let Some(end) = block[..length].iter().position(|&byte| byte == 0) {
                bytes.extend_from_slice(&block[..end]);
                return Ok(bytes);
            }
            bytes.extend_from_slice(&block[..length]);
        }
        Err(Error::Host("unterminated guest string".into()))
    }

    pub fn heap_alloc(&mut self, size: u32) -> Result<u32> {
        self.heap
            .allocate(size, &mut self.memory, &mut self.regions, self.limit)
    }
    pub fn heap_free(&mut self, address: u32) -> Result<()> {
        self.heap.free(address)
    }
    pub fn heap_size(&self, address: u32) -> Result<u32> {
        self.heap.size(address)
    }

    /// Failed growth leaves the original allocation live and its bytes unchanged.
    pub fn heap_realloc(&mut self, address: u32, size: u32) -> Result<u32> {
        if address == 0 {
            return self.heap_alloc(size);
        }
        let old_size = self.heap.size(address)?;
        match self.heap.resize(
            address,
            size,
            &mut self.memory,
            &mut self.regions,
            self.limit,
        ) {
            Ok(true) => return Ok(address),
            // A backing-memory growth failure can still leave a reusable free
            // block elsewhere. Other backing errors remain real failures.
            Ok(false) | Err(Error::OutOfMemory) => {}
            Err(error) => return Err(error),
        }
        let replacement = self.heap_alloc(size)?;
        if let Err(error) = self.memory.copy(replacement, address, size.min(old_size)) {
            self.heap.free(replacement)?;
            return Err(error);
        }
        self.heap.free(address)?;
        Ok(replacement)
    }
}
