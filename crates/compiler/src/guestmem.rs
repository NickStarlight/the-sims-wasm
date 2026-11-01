//! Sparse translation input with a 2 GiB logical address space.
//! Unallocated 64 KiB pages read as zero; addresses beyond the logical end
//! return None. Loaded image bytes occupy only the pages they touch.
use std::collections::HashMap;

pub const GUEST_SIZE: usize = 32768 * 65536; // 2 GiB logical address space
const PAGE: usize = 65536;

pub struct GuestMem {
    pages: HashMap<usize, Vec<u8>>,
}

impl GuestMem {
    pub fn new() -> Self {
        GuestMem {
            pages: HashMap::new(),
        }
    }

    /// Byte at `off`, or None past the logical end.
    #[inline]
    pub fn get(&self, off: usize) -> Option<u8> {
        if off >= GUEST_SIZE {
            return None;
        }
        let page = off / PAGE;
        match self.pages.get(&page) {
            Some(p) => Some(p[off % PAGE]),
            None => Some(0),
        }
    }

    /// Read a signed little-endian word; bytes beyond the logical end contribute zero.
    #[inline]
    pub fn i32_le(&self, off: usize) -> i32 {
        let b = |k: usize| self.get(k).unwrap_or(0) as u32;
        (b(off) | (b(off + 1) << 8) | (b(off + 2) << 16) | (b(off + 3) << 24)) as i32
    }

    /// Unsigned dword read (switchTargets / pointer scans; >>> 0 semantics).
    #[inline]
    pub fn u32_le(&self, off: usize) -> u32 {
        self.i32_le(off) as u32
    }

    /// copyIn — write bytes at guest VA.
    pub fn write(&mut self, va: usize, src: &[u8]) {
        let mut pos = va;
        let mut cur = 0;
        while cur < src.len() {
            let page = pos / PAGE;
            let slot = pos % PAGE;
            let n = std::cmp::min(PAGE - slot, src.len() - cur);
            let p = self.pages.entry(page).or_insert_with(|| vec![0u8; PAGE]);
            p[slot..slot + n].copy_from_slice(&src[cur..cur + n]);
            pos += n;
            cur += n;
        }
    }

    /// Read-modify-write a dword (applyRelocations' sink.setU32).
    pub fn write_u32(&mut self, va: usize, v: u32) {
        self.write(va, &v.to_le_bytes());
    }
}
