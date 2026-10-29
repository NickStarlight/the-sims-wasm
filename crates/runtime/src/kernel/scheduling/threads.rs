//! Thread handles and execution lifetimes remain separate; stacks live until process teardown.
use super::*;
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionDescriptor {
    pub id: u32,
    pub entry: u32,
    pub parameter: u32,
    pub teb: u32,
    pub stack_limit: u32,
    pub stack_base: u32,
}
struct Execution {
    descriptor: ExecutionDescriptor,
    handle: u32,
    suspended: u32,
    started: bool,
    exit: Option<u32>,
}
#[derive(Default)]
pub(in crate::kernel) struct Executions {
    records: BTreeMap<u32, Execution>,
    next: u32,
}
impl Kernel {
    pub fn take_runnable_threads(&mut self) -> Vec<ExecutionDescriptor> {
        self.executions
            .records
            .values_mut()
            .filter(|r| !r.started && r.suspended == 0 && r.exit.is_none())
            .map(|r| {
                r.started = true;
                r.descriptor.clone()
            })
            .collect()
    }
    pub fn finish_thread(&mut self, id: u32, code: u32) -> Result<()> {
        if id == 1 {
            self.events.abandon(id);
            return Ok(());
        }
        let record = self
            .executions
            .records
            .get_mut(&id)
            .ok_or(Error::InvalidAllocation)?;
        if record.exit.is_none() {
            record.exit = Some(code);
            self.events.signal(record.handle);
            self.events.abandon(id);
        }
        Ok(())
    }
    pub(in crate::kernel) fn thread_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        call: Call,
        a: &[u32],
    ) -> Result<u32> {
        if call == Call::CreateThread {
            return self.create_thread(p, a);
        }
        if call == Call::CreatePipe {
            let read = self.events.create(true, false)?;
            let write = self.events.create(true, false)?;
            p.write(a[0], &read.to_le_bytes())?;
            p.write(a[1], &write.to_le_bytes())?;
            return Ok(1);
        }
        if call == Call::DuplicateHandle {
            if a[3] != 0 {
                p.write(a[3], &a[1].to_le_bytes())?;
            }
            return Ok(1);
        }
        if call == Call::TerminateThread {
            return Ok(self.fail(120));
        }
        let Some(record) = self
            .executions
            .records
            .values_mut()
            .find(|r| r.handle == a[0] && self.events.live(a[0]))
        else {
            self.fail(6);
            return Ok(if call == Call::GetExitCodeThread {
                0
            } else {
                u32::MAX
            });
        };
        match call {
            Call::ResumeThread => {
                let previous = record.suspended;
                record.suspended = record.suspended.saturating_sub(1);
                Ok(previous)
            }
            Call::SuspendThread => {
                if record.started {
                    self.fail(120);
                    return Ok(u32::MAX);
                }
                let previous = record.suspended;
                record.suspended = previous.checked_add(1).ok_or(Error::InvalidRange)?;
                Ok(previous)
            }
            Call::GetExitCodeThread => {
                let code = record.exit.unwrap_or(259);
                if a[1] == 0 {
                    return Ok(self.fail(87));
                }
                p.write(a[1], &code.to_le_bytes())?;
                Ok(1)
            }
            _ => Err(Error::Host("incorrect thread routing".into())),
        }
    }
    fn create_thread<M: GuestMemory>(&mut self, p: &mut Process<M>, a: &[u32]) -> Result<u32> {
        if a[4] & !(4 | 0x10000) != 0 || a[1] > 64 * 1024 * 1024 || a[2] == 0 {
            return Ok(self.fail(87));
        }
        if self.executions.records.len() >= 1024 {
            return Ok(self.fail(8));
        }
        let size = if a[1] == 0 { 1 << 20 } else { a[1].max(4096) }
            .checked_add(4095)
            .ok_or(Error::InvalidRange)?
            & !4095;
        let stack_limit = p.virtual_alloc(0, size)?;
        let teb = p.virtual_alloc(0, 4096)?;
        let id = self
            .executions
            .next
            .max(1)
            .checked_add(1)
            .ok_or(Error::OutOfMemory)?;
        let stack_base = stack_limit.checked_add(size).ok_or(Error::InvalidRange)?;
        for (offset, value) in [
            (0, u32::MAX),
            (4, stack_base),
            (8, stack_limit),
            (0x18, teb),
            (0x20, 0x1234),
            (0x24, id),
        ] {
            p.write(teb + offset, &value.to_le_bytes())?;
        }
        if a[5] != 0 {
            p.write(a[5], &id.to_le_bytes())?;
        }
        let handle = self.events.create(true, false)?;
        self.executions.next = id;
        self.executions.records.insert(
            id,
            Execution {
                descriptor: ExecutionDescriptor {
                    id,
                    entry: a[2],
                    parameter: a[3],
                    teb,
                    stack_limit,
                    stack_base,
                },
                handle,
                suspended: u32::from(a[4] & 4 != 0),
                started: false,
                exit: None,
            },
        );
        Ok(handle)
    }
}
