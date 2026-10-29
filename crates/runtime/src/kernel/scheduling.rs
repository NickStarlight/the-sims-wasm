//! Cooperative execution descriptors and waits. Hosts own JSPI stacks, never wait results.
#[cfg(test)]
#[path = "../../tests/unit/kernel/scheduling.rs"]
mod tests;
mod threads;
pub use super::events::ReadyWait;
use super::*;
use serde::Serialize;
pub use threads::ExecutionDescriptor;
pub(super) use threads::Executions;

#[derive(Debug, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ScheduleOutcome {
    Return {
        value: u32,
    },
    Wait {
        token: u32,
        #[serde(rename = "deadlineMs")]
        deadline_ms: Option<f64>,
    },
    Exit {
        code: u32,
    },
}
impl Kernel {
    pub fn schedule_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        id: u32,
        a: &[u32],
        now_ms: f64,
    ) -> Result<ScheduleOutcome> {
        let api = manifest().get(id as usize).ok_or(Error::InvalidRange)?;
        if !api.schedule || a.len() != api.arity as usize {
            return Err(Error::Host("invalid scheduled API call".into()));
        }
        self.advance_time(now_ms)?;
        match api.call {
            Call::ExitThread => Ok(ScheduleOutcome::Exit { code: a[0] }),
            Call::EnterCriticalSection => {
                let lock = self
                    .locks
                    .get_mut(&a[0])
                    .ok_or_else(|| Error::Host("unknown critical section".into()))?;
                if lock.owner.is_none() || lock.owner == Some(self.thread_id) {
                    lock.owner = Some(self.thread_id);
                    lock.depth = lock.depth.checked_add(1).ok_or(Error::InvalidRange)?;
                    return Ok(ScheduleOutcome::Return { value: 0 });
                }
                let token = self.events.lock_wait(self.thread_id)?;
                lock.queue.push_back((self.thread_id, token));
                Ok(ScheduleOutcome::Wait {
                    token,
                    deadline_ms: None,
                })
            }
            Call::Sleep | Call::SleepEx => {
                self.events
                    .begin(self.thread_id, Vec::new(), false, a[0], now_ms, true)
            }
            _ => {
                let (objects, all, timeout) = match api.call {
                    Call::WaitForSingleObject | Call::WaitForSingleObjectEx => {
                        (vec![a[0]], false, a[1])
                    }
                    Call::WaitForMultipleObjects | Call::WaitForMultipleObjectsEx => {
                        if a[0] == 0 || a[0] > 64 {
                            self.fail(87);
                            return Ok(ScheduleOutcome::Return { value: u32::MAX });
                        }
                        let mut bytes = vec![0; a[0] as usize * 4];
                        p.read(a[1], &mut bytes)?;
                        (
                            bytes
                                .as_chunks::<4>()
                                .0
                                .iter()
                                .map(|b| u32::from_le_bytes(*b))
                                .collect(),
                            a[2] != 0,
                            a[3],
                        )
                    }
                    _ => return Err(Error::Host("unsupported scheduled call".into())),
                };
                match self
                    .events
                    .begin(self.thread_id, objects, all, timeout, now_ms, false)
                {
                    Err(Error::InvalidAllocation) => {
                        self.fail(6);
                        Ok(ScheduleOutcome::Return { value: u32::MAX })
                    }
                    result => result,
                }
            }
        }
    }
    pub fn advance_time(&mut self, now_ms: f64) -> Result<()> {
        if !now_ms.is_finite() || now_ms < 0.0 {
            return Err(Error::InvalidRange);
        }
        self.events.advance(now_ms);
        Ok(())
    }
    /// Read only already-completed waits. This neither advances time nor consumes results.
    pub fn has_ready_waits(&self) -> bool {
        self.events.has_ready()
    }
    pub fn take_ready_waits(&mut self) -> Vec<ReadyWait> {
        self.events.take_ready()
    }
    pub fn cancel_wait(&mut self, token: u32) -> bool {
        for lock in self.locks.values_mut() {
            lock.queue.retain(|(_, t)| *t != token);
        }
        self.events.cancel(token)
    }
}
