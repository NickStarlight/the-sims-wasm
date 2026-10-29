//! Original 16-slot deadline table; advance before callbacks so reentry and cancellation are safe.
use super::*;
#[derive(Clone, Copy)]
struct Timer {
    id: u32,
    delay: u32,
    callback: u32,
    user: u32,
    flags: u32,
    next: u32,
}
pub(super) struct Timers {
    slots: [Option<Timer>; 16],
    next_id: u32,
}
impl Default for Timers {
    fn default() -> Self {
        Self {
            slots: [None; 16],
            next_id: 0xe0000000,
        }
    }
}
impl Timers {
    pub(super) fn create(
        &mut self,
        delay: u32,
        callback: u32,
        user: u32,
        flags: u32,
        now: u32,
    ) -> u32 {
        if Media::period(delay) != 0 {
            return 0;
        }
        let Some(slot) = self.slots.iter().position(Option::is_none) else {
            return 0;
        };
        let id = self.next_id;
        self.next_id = self.next_id.wrapping_add(1);
        self.slots[slot] = Some(Timer {
            id,
            delay,
            callback,
            user,
            flags,
            next: now.wrapping_add(delay),
        });
        id
    }
    pub(super) fn kill(&mut self, id: u32) -> u32 {
        let Some(slot) = self
            .slots
            .iter()
            .position(|timer| timer.is_some_and(|timer| timer.id == id))
        else {
            return 97;
        };
        self.slots[slot] = None;
        0
    }
    pub(super) fn next(&mut self, now: u32) -> Option<MediaCallback> {
        loop {
            let slot = self.slots.iter().position(|timer| {
                timer.is_some_and(|timer| (now.wrapping_sub(timer.next) as i32) >= 0)
            })?;
            let timer = self.slots[slot].unwrap();
            self.slots[slot] = if timer.flags & 1 != 0 {
                Some(Timer {
                    next: timer.next.wrapping_add(timer.delay),
                    ..timer
                })
            } else {
                None
            };
            // The old host invokes function timers only. Event-mode timers advance
            // without synthesizing unsupported event signaling.
            if timer.flags & 0x30 == 0 {
                return Some(MediaCallback {
                    address: timer.callback,
                    args: [timer.id, 0, timer.user, 0, 0],
                });
            }
        }
    }
}
