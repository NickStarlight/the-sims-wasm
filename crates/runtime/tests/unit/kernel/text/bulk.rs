use super::*;
use crate::SparseMemory;
#[test]
fn bulk_text_stops_at_nul_before_unmapped_memory() {
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
    p.map(0x200000, b"abc\0", false).unwrap();
    assert_eq!(
        read_units(&p, 0x200000, 1_048_576, false, true).unwrap(),
        [97, 98, 99, 0]
    );
    p.map(0x300001, &[0x34, 0x12, 0, 0], false).unwrap();
    assert_eq!(
        read_units(&p, 0x300001, 1_048_576, true, true).unwrap(),
        [0x1234, 0]
    );
    assert!(read_units(&p, 0x200000, 5, false, false).is_err());
    assert_eq!(
        read_units(&p, 0x200000, 3, false, false).unwrap(),
        [97, 98, 99]
    );
}
#[test]
fn bulk_text_preserves_embedded_nuls_and_chunk_boundaries() {
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
    let mut bytes = vec![42; 1025];
    bytes[255] = 0;
    bytes[1024] = 0;
    p.map(0x200000, &bytes, false).unwrap();
    assert_eq!(
        read_units(&p, 0x200000, 1025, false, false).unwrap(),
        bytes.iter().map(|b| *b as u16).collect::<Vec<_>>()
    );
    assert_eq!(
        read_units(&p, 0x200000, 1025, false, true).unwrap().len(),
        256
    );
}

#[test]
fn bulk_text_reduces_guest_transfers() {
    use std::{cell::Cell, rc::Rc};
    struct Counted {
        memory: SparseMemory,
        reads: Rc<Cell<usize>>,
    }
    impl GuestMemory for Counted {
        fn ensure(&mut self, end: u64) -> Result<()> {
            self.memory.ensure(end)
        }
        fn read(&self, at: u32, bytes: &mut [u8]) -> Result<()> {
            self.reads.set(self.reads.get() + 1);
            self.memory.read(at, bytes)
        }
        fn write(&mut self, at: u32, bytes: &[u8]) -> Result<()> {
            self.memory.write(at, bytes)
        }
        fn fill(&mut self, at: u32, size: u32, value: u8) -> Result<()> {
            self.memory.fill(at, size, value)
        }
        fn copy(&mut self, dest: u32, source: u32, size: u32) -> Result<()> {
            self.memory.copy(dest, source, size)
        }
    }
    let reads = Rc::new(Cell::new(0));
    let mut p = Process::new(
        Counted {
            memory: SparseMemory::default(),
            reads: reads.clone(),
        },
        0x10000,
        0x1000000,
    )
    .unwrap();
    let mut bytes = vec![42; 1025];
    bytes[1024] = 0;
    p.map(0x200000, &bytes, false).unwrap();
    assert_eq!(
        read_units(&p, 0x200000, 1_048_576, false, true)
            .unwrap()
            .len(),
        1025
    );
    assert_eq!(reads.get(), 5);
}
