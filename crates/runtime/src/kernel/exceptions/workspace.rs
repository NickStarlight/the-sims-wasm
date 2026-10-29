//! x86 exception records and CONTEXT layout from the original SEH bridge.
use super::*;
pub(super) const ESP: usize = 4;
pub(super) const ACTUAL_RETURN: usize = 14;
pub(super) const TEB: usize = 15;
const CONTEXT_REGISTERS: [(usize, u32); 9] = [
    (7, 0x9c),
    (6, 0xa0),
    (3, 0xa4),
    (2, 0xa8),
    (1, 0xac),
    (0, 0xb0),
    (5, 0xb4),
    (8, 0xc0),
    (4, 0xc4),
];
pub(super) struct Workspace {
    pub saved: [u32; 16],
    pub sp: u32,
    pub base: u32,
    pub context: u32,
    pub dispatcher: u32,
    limit: u32,
    end: u32,
}
impl Workspace {
    pub fn new<M: GuestMemory>(p: &mut Process<M>, saved: [u32; 16]) -> Result<Self> {
        let sp = saved[ESP];
        let base = sp.checked_sub(0x400).ok_or(Error::InvalidRange)? & !15;
        let w = Self {
            saved,
            sp,
            base,
            context: base + 0x80,
            dispatcher: base + 0x350,
            limit: word(p, saved[TEB] + 8)?,
            end: word(p, saved[TEB] + 4)?,
        };
        w.validate(base, 0x400)?;
        p.write(base, &[0; 0x400])?;
        put(p, w.context, 0x10007)?;
        for (reg, offset) in CONTEXT_REGISTERS {
            put(p, w.context + offset, saved[reg])?;
        }
        put(p, w.context + 0xb8, word(p, sp)?)?;
        put(p, w.context + 0xc4, sp + 20)?;
        Ok(w)
    }
    pub fn validate(&self, frame: u32, bytes: u32) -> Result<()> {
        if frame & 3 != 0
            || self.limit >= self.end
            || frame < self.limit
            || u64::from(frame) + u64::from(bytes) > u64::from(self.end)
        {
            return Err(failure("invalid exception stack frame"));
        }
        Ok(())
    }
    pub fn handler<M: GuestMemory>(
        &self,
        p: &mut Process<M>,
        frame: u32,
        record: u32,
    ) -> Result<(u32, [u32; 16])> {
        self.validate(frame, 8)?;
        let address = word(p, frame + 4)?;
        if address == 0 {
            return Err(failure("null exception handler"));
        }
        let stack = self.base.checked_sub(20).ok_or(Error::InvalidRange)?;
        self.validate(stack, 20)?;
        // The handler is cdecl; its four arguments are discarded with this workspace.
        for (i, value) in [0, record, frame, self.context, self.dispatcher]
            .into_iter()
            .enumerate()
        {
            put(p, stack + i as u32 * 4, value)?;
        }
        let mut registers = self.saved;
        registers[ESP] = stack;
        registers[ACTUAL_RETURN] = u32::MAX;
        registers[13] = 0;
        Ok((address, registers))
    }
    pub fn finish<M: GuestMemory>(&self, p: &Process<M>, value: u32) -> Result<[u32; 16]> {
        let mut registers = self.saved;
        registers[0] = value;
        registers[ESP] = self.sp + 20;
        registers[ACTUAL_RETURN] = word(p, self.sp)?;
        Ok(registers)
    }
    pub fn continued<M: GuestMemory>(&self, p: &Process<M>) -> Result<[u32; 16]> {
        if word(p, self.context + 0xb8)? != word(p, self.sp)?
            || word(p, self.context + 0xc4)? != self.sp + 20
        {
            return Err(failure("edited continuation context is not supported"));
        }
        let mut registers = self.finish(p, word(p, self.context + 0xb0)?)?;
        for (reg, offset) in CONTEXT_REGISTERS {
            registers[reg] = word(p, self.context + offset)?;
        }
        Ok(registers)
    }
}
pub(super) fn word<M: GuestMemory>(p: &Process<M>, address: u32) -> Result<u32> {
    let mut bytes = [0; 4];
    p.read(address, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
pub(super) fn put<M: GuestMemory>(p: &mut Process<M>, address: u32, value: u32) -> Result<()> {
    p.write(address, &value.to_le_bytes())
}
