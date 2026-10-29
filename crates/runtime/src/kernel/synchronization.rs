//! Thread-local storage and reentrant critical-section ownership.
use super::*;

impl Kernel {
    /// Atomic relative to the single guest execution worker: this call never suspends.
    /// This is not a shared-memory host atomic and must change before parallel guest threads exist.
    pub(super) fn interlocked<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        if args[0] & 3 != 0 {
            return Err(Error::Host(
                "Interlocked operation requires a DWORD-aligned address".into(),
            ));
        }
        use crate::U32Update;
        let operation = match call {
            Call::InterlockedIncrement => U32Update::Add(1),
            Call::InterlockedDecrement => U32Update::Add(u32::MAX),
            Call::InterlockedExchange => U32Update::Exchange(args[1]),
            Call::InterlockedExchangeAdd => U32Update::Add(args[1]),
            Call::InterlockedCompareExchange => U32Update::CompareExchange {
                exchange: args[1],
                comparand: args[2],
            },
            _ => return Err(Error::Host("incorrect Interlocked service routing".into())),
        };
        // Validate writable ownership even when compare-exchange leaves the value unchanged.
        let previous = process.update_u32(args[0], operation)?;
        Ok(
            if matches!(
                call,
                Call::InterlockedIncrement | Call::InterlockedDecrement
            ) {
                operation.apply(previous)
            } else {
                previous
            },
        )
    }

    pub(super) fn synchronization<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let result = match call {
            Call::SetThreadPriority | Call::GetThreadPriority => {
                if args[0] != 0xffff_fffe {
                    self.fail(6); // ERROR_INVALID_HANDLE; only the current-thread pseudohandle exists.
                    return Ok(if call == Call::GetThreadPriority {
                        0x7fff_ffff
                    } else {
                        0
                    });
                }
                if call == Call::GetThreadPriority {
                    return Ok(self.thread().priority as u32);
                }
                let priority = args[1] as i32;
                if matches!(priority, 0x10000 | 0x20000) {
                    return Err(Error::Host(
                        "thread background resource scheduling is unsupported".into(),
                    ));
                }
                if !matches!(priority, -15 | -2..=2 | 15) {
                    return Ok(self.fail(87));
                }
                // One runnable guest thread: store Windows priority state, but do not
                // claim control over browser/OS scheduling or create native threads.
                self.thread().priority = priority;
                1
            }
            Call::InitializeCriticalSection => {
                process.write(args[0], &[0; 24])?;
                self.locks.insert(args[0], CriticalSection::default());
                0
            }
            Call::DeleteCriticalSection => {
                if self.locks.remove(&args[0]).is_none() {
                    return Err(Error::Host("deleting unknown critical section".into()));
                }
                0
            }
            Call::EnterCriticalSection | Call::TryEnterCriticalSection => {
                let lock = self
                    .locks
                    .get_mut(&args[0])
                    .ok_or_else(|| Error::Host("entering unknown critical section".into()))?;
                if lock.owner.is_some_and(|owner| owner != self.thread_id) {
                    if call == Call::TryEnterCriticalSection {
                        return Ok(0);
                    }
                    return Err(Error::Host(
                        "contended critical section requires scheduler suspension".into(),
                    ));
                }
                lock.owner = Some(self.thread_id);
                lock.depth = lock.depth.checked_add(1).ok_or(Error::InvalidRange)?;
                1
            }
            Call::LeaveCriticalSection => {
                let lock = self
                    .locks
                    .get_mut(&args[0])
                    .ok_or_else(|| Error::Host("leaving unknown critical section".into()))?;
                if lock.owner != Some(self.thread_id) {
                    return Err(Error::Host(
                        "critical section owned by another thread".into(),
                    ));
                }
                lock.depth -= 1;
                if lock.depth == 0 {
                    if let Some((owner, token)) = lock.queue.pop_front() {
                        lock.owner = Some(owner);
                        lock.depth = 1;
                        self.events.complete(token, 0);
                    } else {
                        lock.owner = None;
                    }
                }
                0
            }
            Call::TlsAlloc => {
                if let Some(slot) = (0..64).find(|slot| !self.tls_slots.contains(slot)) {
                    self.tls_slots.insert(slot);
                    slot
                } else {
                    self.fail(8);
                    u32::MAX
                }
            }
            Call::TlsFree => {
                if !self.tls_slots.remove(&args[0]) {
                    return Ok(self.fail(87));
                }
                for thread in self.threads.values_mut() {
                    thread.tls.remove(&args[0]);
                }
                1
            }
            Call::TlsGetValue => {
                if !self.tls_slots.contains(&args[0]) {
                    return Ok(self.fail(87));
                }
                self.thread().error = 0;
                *self.thread().tls.get(&args[0]).unwrap_or(&0)
            }
            Call::TlsSetValue => {
                if !self.tls_slots.contains(&args[0]) {
                    return Ok(self.fail(87));
                }
                self.thread().tls.insert(args[0], args[1]);
                1
            }
            Call::GetCurrentThreadId => self.thread_id,
            Call::GetLastError => self.thread().error,
            Call::SetLastError => {
                self.thread().error = args[0];
                0
            }
            _ => return Err(Error::Host("incorrect kernel service routing".into())),
        };
        Ok(result)
    }
}
