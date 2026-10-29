//! Software exception search/unwind. Guest handlers retain C++ type matching and destructors.
//! Port of the preserved src/runtime/winapi/seh.ts; Rust owns records and continuations.
#[cfg(test)]
#[path = "../../tests/unit/kernel/exceptions.rs"]
mod tests;
mod workspace;
use super::*;
use serde::Serialize;
use workspace::*;
const END: u32 = u32::MAX;
#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ExceptionAction {
    Invoke {
        token: u32,
        address: u32,
        registers: [u32; 16],
    },
    Complete {
        registers: [u32; 16],
    },
}
enum Mode {
    Search { flags: u32 },
    Unwind { target: u32, value: u32 },
}
struct Pending {
    owner: u32,
    workspace: Workspace,
    record: u32,
    frame: u32,
    visited: BTreeSet<u32>,
    mode: Mode,
}
#[derive(Default)]
pub(super) struct Exceptions {
    next: u32,
    pending: BTreeMap<u32, Pending>,
}
fn failure(message: &str) -> Error {
    Error::Host(format!("SEH: {message}"))
}
impl Kernel {
    pub fn begin_exception<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        id: u32,
        args: &[u32],
        registers: &[u32],
    ) -> Result<ExceptionAction> {
        let api = manifest().get(id as usize).ok_or(Error::InvalidRange)?;
        if !api.exception || args.len() != 4 {
            return Err(Error::InvalidRange);
        }
        let saved: [u32; 16] = registers.try_into().map_err(|_| Error::InvalidRange)?;
        let w = Workspace::new(p, saved)?;
        let (record, mode) = if api.call == Call::RaiseException {
            if args[2] > 15 {
                return Err(failure("more than 15 exception parameters"));
            }
            let record = w.base;
            for (offset, value) in [
                (0, args[0]),
                (4, args[1] & 1),
                (12, word(p, w.sp)?),
                (16, args[2]),
            ] {
                put(p, record + offset, value)?;
            }
            for i in 0..args[2] {
                put(
                    p,
                    record + 20 + i * 4,
                    word(p, args[3].checked_add(i * 4).ok_or(Error::InvalidRange)?)?,
                )?;
            }
            (record, Mode::Search { flags: args[1] & 1 })
        } else {
            let record = if args[2] == 0 { w.base } else { args[2] };
            if args[2] == 0 {
                put(p, record, 0xc0000027)?;
                put(p, record + 12, word(p, w.sp)?)?;
            }
            put(
                p,
                record + 4,
                word(p, record + 4)? | 2 | if args[0] == 0 { 4 } else { 0 },
            )?;
            (
                record,
                Mode::Unwind {
                    target: args[0],
                    value: args[3],
                },
            )
        };
        let frame = word(p, saved[TEB])?;
        self.advance_exception(
            p,
            Pending {
                owner: self.thread_id,
                workspace: w,
                record,
                frame,
                visited: BTreeSet::new(),
                mode,
            },
        )
    }
    pub fn resume_exception<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        token: u32,
        registers: &[u32],
    ) -> Result<ExceptionAction> {
        if !self
            .exceptions
            .pending
            .get(&token)
            .is_some_and(|s| s.owner == self.thread_id)
        {
            return Err(failure("unknown or foreign continuation"));
        }
        let registers: [u32; 16] = registers.try_into().map_err(|_| Error::InvalidRange)?;
        let mut s = self.exceptions.pending.remove(&token).unwrap();
        // The CRT can transfer directly to a catch continuation and return through
        // another native frame. Preserve that CPU state; never run the stale shim tail.
        if registers[ACTUAL_RETURN] != 0 {
            return Ok(ExceptionAction::Complete { registers });
        }
        let disposition = registers[0];
        match s.mode {
            Mode::Search { flags } => {
                if disposition == 0 {
                    if flags != 0 {
                        return Err(failure("cannot continue a noncontinuable exception"));
                    }
                    return Ok(ExceptionAction::Complete {
                        registers: s.workspace.continued(p)?,
                    });
                }
                if disposition != 1 {
                    return Err(failure("unsupported search disposition"));
                }
                s.frame = word(p, s.frame)?;
            }
            Mode::Unwind { .. } => {
                if disposition == 3 {
                    s.frame = word(p, s.workspace.dispatcher)?;
                    s.workspace.validate(s.frame, 8)?;
                } else if disposition != 1 {
                    return Err(failure("invalid unwind disposition"));
                }
                s.frame = word(p, s.frame)?;
                put(p, s.workspace.saved[TEB], s.frame)?;
            }
        }
        self.advance_exception(p, s)
    }
    pub fn cancel_exception(&mut self, token: u32) -> bool {
        if self
            .exceptions
            .pending
            .get(&token)
            .is_some_and(|s| s.owner == self.thread_id)
        {
            self.exceptions.pending.remove(&token);
            true
        } else {
            false
        }
    }
    fn advance_exception<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        mut s: Pending,
    ) -> Result<ExceptionAction> {
        if let Mode::Unwind { target, value } = s.mode {
            if s.frame == target || s.frame == END {
                if target != 0 && s.frame != target {
                    return Err(failure("unwind target absent from chain"));
                }
                return Ok(ExceptionAction::Complete {
                    registers: s.workspace.finish(p, value)?,
                });
            }
            if target != 0 && s.frame > target {
                return Err(failure("invalid unwind target"));
            }
        } else if s.frame == END {
            return Err(failure("unhandled guest exception"));
        }
        if s.visited.len() >= 4096 || !s.visited.insert(s.frame) {
            return Err(failure("cyclic or excessive exception chain"));
        }
        if self.exceptions.pending.len() >= 1024 {
            return Err(failure("too many pending exceptions"));
        }
        let (address, registers) = s.workspace.handler(p, s.frame, s.record)?;
        let token = self
            .exceptions
            .next
            .checked_add(1)
            .ok_or(Error::OutOfMemory)?;
        self.exceptions.next = token;
        self.exceptions.pending.insert(token, s);
        Ok(ExceptionAction::Invoke {
            token,
            address,
            registers,
        })
    }
}
