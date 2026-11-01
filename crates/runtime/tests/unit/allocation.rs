use super::*;
use crate::SparseMemory;

#[derive(Default)]
struct FailingMemory {
    inner: SparseMemory,
    fail_ensure: bool,
    fail_fill: bool,
}
impl GuestMemory for FailingMemory {
    fn ensure(&mut self, end: u64) -> Result<()> {
        if self.fail_ensure {
            return Err(Error::OutOfMemory);
        }
        self.inner.ensure(end)
    }
    fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()> {
        self.inner.read(address, bytes)
    }
    fn write(&mut self, address: u32, bytes: &[u8]) -> Result<()> {
        self.inner.write(address, bytes)
    }
    fn fill(&mut self, address: u32, size: u32, value: u8) -> Result<()> {
        if self.fail_fill {
            return Err(Error::OutOfMemory);
        }
        self.inner.fill(address, size, value)
    }
    fn copy(&mut self, destination: u32, source: u32, size: u32) -> Result<()> {
        self.inner.copy(destination, source, size)
    }
}

#[test]
fn failed_resize_keeps_frontier_free_blocks_and_old_contents() {
    for adjacent in [false, true] {
        let mut memory = FailingMemory::default();
        let mut regions = Regions::default();
        let mut heap = Heap::new(0x1000);
        let a = heap
            .allocate(16, &mut memory, &mut regions, 0x10000)
            .unwrap();
        memory.write(a, b"live").unwrap();
        if adjacent {
            let b = heap
                .allocate(32, &mut memory, &mut regions, 0x10000)
                .unwrap();
            heap.free(b).unwrap();
            memory.fail_fill = true;
        } else {
            memory.fail_ensure = true;
        }
        let region_count = regions.values().count();
        assert_eq!(
            heap.resize(a, 48, &mut memory, &mut regions, 0x10000),
            Err(Error::OutOfMemory)
        );
        assert_eq!(heap.size(a).unwrap(), 16);
        assert_eq!(regions.values().count(), region_count);
        let mut bytes = [0; 4];
        memory.read(a, &mut bytes).unwrap();
        assert_eq!(&bytes, b"live");
        memory.fail_ensure = false;
        memory.fail_fill = false;
        assert_eq!(
            heap.allocate(32, &mut memory, &mut regions, 0x10000)
                .unwrap(),
            a + 16
        );
    }
}
