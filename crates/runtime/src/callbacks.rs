//! Owned continuations. A returned callback is executed only after the Rust borrow ends.
use crate::{Error, GuestMemory, Process, Result};
use serde::Serialize;
use std::collections::BTreeMap;

const MAX_INITIALIZERS: u32 = 16_384;
const MAX_PENDING_CALLS: usize = 1024;

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CallOutcome {
    Return {
        value: u32,
    },
    Callback {
        address: u32,
        args: Vec<u32>,
        token: u32,
    },
}

struct Initializers {
    next: u32,
    end: u32,
}

enum PendingCall {
    Initializers(Initializers),
    Sequence {
        calls: std::collections::VecDeque<(u32, Vec<u32>)>,
        stop_on_zero: bool,
        stop_mask: u32,
        stop_value: u32,
        value: u32,
    },
    User(crate::user::Continuation),
}

#[derive(Default)]
pub(crate) struct Callbacks {
    next_token: u32,
    pending: BTreeMap<u32, PendingCall>,
}

impl Callbacks {
    pub(crate) fn initializers<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        start: u32,
        end: u32,
    ) -> Result<CallOutcome> {
        let size = end.checked_sub(start).ok_or(Error::InvalidRange)?;
        if size % 4 != 0 || size / 4 > MAX_INITIALIZERS {
            return Err(Error::InvalidRange);
        }
        self.advance(
            process,
            PendingCall::Initializers(Initializers { next: start, end }),
        )
    }

    pub(crate) fn resume<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        token: u32,
        _result: u32,
    ) -> Result<CallOutcome> {
        // Consume first: stale/replayed tokens cannot advance a call twice, and
        // validation errors cannot strand its suspended continuation.
        let pending = self
            .pending
            .remove(&token)
            .ok_or_else(|| Error::Host(format!("unknown or consumed callback token {token}")))?;
        if let PendingCall::Sequence {
            stop_on_zero: true,
            stop_mask,
            stop_value,
            ..
        } = &pending
        {
            if _result & stop_mask == 0 {
                return Ok(CallOutcome::Return { value: *stop_value });
            }
        }
        // _initterm callback return values are deliberately ignored.
        self.advance(process, pending)
    }

    pub(crate) fn cancel(&mut self, token: u32) -> bool {
        self.pending.remove(&token).is_some()
    }

    fn advance<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        pending: PendingCall,
    ) -> Result<CallOutcome> {
        match pending {
            PendingCall::User(_) => Err(Error::Host(
                "window callback requires USER32 continuation routing".into(),
            )),
            PendingCall::Sequence {
                mut calls,
                stop_on_zero,
                stop_mask,
                stop_value,
                value,
            } => {
                if let Some((address, args)) = calls.pop_front() {
                    let token = self.insert(PendingCall::Sequence {
                        calls,
                        stop_on_zero,
                        stop_mask,
                        stop_value,
                        value,
                    })?;
                    Ok(CallOutcome::Callback {
                        address,
                        args,
                        token,
                    })
                } else {
                    Ok(CallOutcome::Return { value })
                }
            }
            PendingCall::Initializers(mut state) => {
                if let Some(address) = next_initializer(process, &mut state)? {
                    let token = self.insert(PendingCall::Initializers(state))?;
                    return Ok(CallOutcome::Callback {
                        address,
                        args: Vec::new(),
                        token,
                    });
                }
                Ok(CallOutcome::Return { value: 0 })
            }
        }
    }

    pub(crate) fn sequence(
        &mut self,
        calls: Vec<(u32, Vec<u32>)>,
        stop_on_zero: bool,
        value: u32,
    ) -> Result<CallOutcome> {
        self.sequence_with_stop(calls, stop_on_zero, value, u32::MAX, value)
    }

    pub(crate) fn sequence_with_stop(
        &mut self,
        calls: Vec<(u32, Vec<u32>)>,
        stop_on_zero: bool,
        value: u32,
        stop_mask: u32,
        stop_value: u32,
    ) -> Result<CallOutcome> {
        if calls.len() > MAX_INITIALIZERS as usize
            || calls
                .iter()
                .any(|(address, args)| *address == 0 || args.len() > 16)
        {
            return Err(Error::InvalidRange);
        }
        let mut calls: std::collections::VecDeque<_> = calls.into();
        let Some((address, args)) = calls.pop_front() else {
            return Ok(CallOutcome::Return { value });
        };
        let token = self.insert(PendingCall::Sequence {
            calls,
            stop_on_zero,
            stop_mask,
            stop_value,
            value,
        })?;
        Ok(CallOutcome::Callback {
            address,
            args,
            token,
        })
    }

    pub(crate) fn user_action(
        &mut self,
        user: &mut crate::user::User,
        action: crate::user::Action,
    ) -> Result<CallOutcome> {
        match action {
            crate::user::Action::Return(value) => Ok(CallOutcome::Return { value }),
            crate::user::Action::Invoke {
                address,
                args,
                next,
            } => {
                // Keep cleanup ownership if token capacity is exhausted.
                if self.pending.len() >= MAX_PENDING_CALLS || self.next_token == u32::MAX {
                    user.cancel(next);
                    return Err(Error::Host("callback token capacity exhausted".into()));
                }
                let token = self.insert(PendingCall::User(next))?;
                Ok(CallOutcome::Callback {
                    address,
                    args,
                    token,
                })
            }
        }
    }
    pub(crate) fn resume_user<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        user: &mut crate::user::User,
        token: u32,
        result: u32,
    ) -> Result<CallOutcome> {
        if matches!(self.pending.get(&token), Some(PendingCall::User(_))) {
            let Some(PendingCall::User(next)) = self.pending.remove(&token) else {
                unreachable!()
            };
            let action = user.resume_with_memory(process, next, result)?;
            self.user_action(user, action)
        } else {
            self.resume(process, token, result)
        }
    }
    pub(crate) fn cancel_user(&mut self, user: &mut crate::user::User, token: u32) -> bool {
        if matches!(self.pending.get(&token), Some(PendingCall::User(_))) {
            if let Some(PendingCall::User(next)) = self.pending.remove(&token) {
                user.cancel(next);
            }
            true
        } else {
            self.cancel(token)
        }
    }

    fn insert(&mut self, pending: PendingCall) -> Result<u32> {
        if self.pending.len() >= MAX_PENDING_CALLS {
            return Err(Error::Host("pending callback limit exceeded".into()));
        }
        let token = self
            .next_token
            .checked_add(1)
            .ok_or_else(|| Error::Host("callback token space exhausted".into()))?;
        self.next_token = token;
        self.pending.insert(token, pending);
        Ok(token)
    }
}

/// Batch null runs, but defer later pointer reads until after the current callback.
fn next_initializer<M: GuestMemory>(
    process: &Process<M>,
    state: &mut Initializers,
) -> Result<Option<u32>> {
    while state.next < state.end {
        let mut bytes = [0; 256];
        let mut length = (state.end - state.next).min(bytes.len() as u32) as usize;
        match process.read(state.next, &mut bytes[..length]) {
            Ok(()) => {}
            Err(Error::Unmapped | Error::Protected | Error::InvalidRange) => {
                length = 4;
                process.read(state.next, &mut bytes[..length])?;
            }
            Err(error) => return Err(error),
        }
        for bytes in bytes[..length].as_chunks::<4>().0 {
            state.next += 4;
            let address = u32::from_le_bytes(*bytes);
            if address != 0 {
                return Ok(Some(address));
            }
        }
    }
    Ok(None)
}

#[cfg(test)]
#[path = "../tests/unit/callbacks.rs"]
mod tests;
