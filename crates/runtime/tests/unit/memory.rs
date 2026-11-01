use super::*;

struct ReadOnlyBacking(SparseMemory);
impl GuestMemory for ReadOnlyBacking {
    fn ensure(&mut self, end: u64) -> Result<()> {
        self.0.ensure(end)
    }
    fn read(&self, address: u32, bytes: &mut [u8]) -> Result<()> {
        self.0.read(address, bytes)
    }
    fn write(&mut self, _: u32, _: &[u8]) -> Result<()> {
        Err(Error::Protected)
    }
    fn fill(&mut self, _: u32, _: u32, _: u8) -> Result<()> {
        Err(Error::Protected)
    }
    fn copy(&mut self, _: u32, _: u32, _: u32) -> Result<()> {
        Err(Error::Protected)
    }
}

#[test]
fn default_dword_updates_keep_backing_errors_and_full_address_space() {
    let mut memory = SparseMemory::default();
    memory.ensure(1u64 << 32).unwrap();
    memory.write(0xffff_fffc, &u32::MAX.to_le_bytes()).unwrap();
    assert_eq!(
        memory.update_u32(0xffff_fffc, U32Update::Add(1)),
        Ok(u32::MAX)
    );
    let mut bytes = [0; 4];
    memory.read(0xffff_fffc, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
    assert_eq!(
        memory.update_u32(0xffff_fffd, U32Update::Exchange(7)),
        Err(Error::Unmapped)
    );
    let mut memory = ReadOnlyBacking(memory);
    assert_eq!(
        memory.update_u32(
            0xffff_fffc,
            U32Update::CompareExchange {
                exchange: 8,
                comparand: 9,
            }
        ),
        Err(Error::Protected),
        "a failed comparison still attempts the backing write"
    );
    memory.read(0xffff_fffc, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
}
