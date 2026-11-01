//! Multimedia behavior ported from runtime-rs/src/media and winapi/misc.ts.
//! Timers are pumped at MMIO entry, not by a fabricated background guest thread.
mod mmio;
#[cfg(test)]
#[path = "../../tests/unit/media/mod.rs"]
mod tests;
mod timers;
use crate::{Error, GuestMemory, Process, Result};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Default)]
pub(crate) struct Media {
    files: BTreeMap<u32, mmio::File>,
    timers: timers::Timers,
}
#[derive(Serialize)]
pub struct MediaCallback {
    pub address: u32,
    pub args: [u32; 5],
}
impl Media {
    pub(crate) fn period(period: u32) -> u32 {
        if (1..=65535).contains(&period) {
            0
        } else {
            97
        }
    }
    pub(crate) fn create_timer(
        &mut self,
        delay: u32,
        callback: u32,
        user: u32,
        flags: u32,
        now: u32,
    ) -> u32 {
        self.timers.create(delay, callback, user, flags, now)
    }
    pub(crate) fn kill_timer(&mut self, id: u32) -> u32 {
        self.timers.kill(id)
    }
    pub(crate) fn next_timer(&mut self, now: u32) -> Option<MediaCallback> {
        self.timers.next(now)
    }
}
