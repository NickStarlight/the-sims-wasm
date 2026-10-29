//! Keyboard transitions ported from runtime-rs/src/user/input.rs.
pub(super) struct Keyboard {
    pub physical: [u8; 256],
    pub queued: [u8; 256],
    pub pressed: [u8; 256],
}
impl Keyboard {
    /// Translate the queued key state using the runtime's US keyboard layout.
    /// Physical state may already include later browser events in the queue.
    pub fn character(&self, vk: u32) -> Option<u32> {
        let shift = self.queued[0x10] & 0x80 != 0;
        let caps = self.queued[0x14] & 1 != 0;
        let control = self.queued[0x11] & 0x80 != 0;
        if control {
            return match vk {
                0x41..=0x5a => Some(vk - 0x40),
                0x08 => Some(127),
                0xdb => Some(27),
                0xdc => Some(28),
                0xdd => Some(29),
                _ => None,
            };
        }
        Some(match vk {
            0x41..=0x5a => {
                if shift ^ caps {
                    vk
                } else {
                    vk + 32
                }
            }
            0x30..=0x39 => {
                if shift {
                    b")!@#$%^&*("[(vk - 0x30) as usize] as u32
                } else {
                    vk
                }
            }
            0x08 | 0x09 | 0x0d | 0x1b | 0x20 => vk,
            0x60..=0x69 => vk - 0x60 + u32::from(b'0'),
            0x6a => u32::from(b'*'),
            0x6b => u32::from(b'+'),
            0x6d => u32::from(b'-'),
            0x6e => u32::from(b'.'),
            0x6f => u32::from(b'/'),
            _ => {
                let pair = match vk {
                    0xba => b";:",
                    0xbb => b"=+",
                    0xbc => b",<",
                    0xbd => b"-_",
                    0xbe => b".>",
                    0xbf => b"/?",
                    0xc0 => b"`~",
                    0xdb => b"[{",
                    0xdc => b"\\|",
                    0xdd => b"]}",
                    0xde => b"'\"",
                    _ => return None,
                };
                pair[usize::from(shift)] as u32
            }
        })
    }
    pub const fn new() -> Self {
        Self {
            physical: [0; 256],
            queued: [0; 256],
            pressed: [0; 256],
        }
    }
    pub fn update(&mut self, msg: u32, wp: u32, lp: u32, physical: bool) {
        let state = if physical {
            &mut self.physical
        } else {
            &mut self.queued
        };
        let mut change = |key: u32, down: bool| {
            if key >= 256 {
                return;
            }
            let key = key as usize;
            let was = state[key] & 0x80 != 0;
            let toggle = matches!(key, 0x14 | 0x90 | 0x91) && down && !was;
            state[key] = if down { 0x80 } else { 0 } | ((state[key] & 1) ^ toggle as u8);
            if physical && down && !was {
                self.pressed[key] = 1;
            }
        };
        if matches!(msg, 0x100 | 0x101 | 0x104 | 0x105) {
            let key = match wp {
                0x10 => {
                    if (lp >> 16) & 255 == 0x36 {
                        0xa1
                    } else {
                        0xa0
                    }
                }
                0x11 | 0x12 => (if wp == 0x11 { 0xa2 } else { 0xa4 }) + ((lp >> 24) & 1),
                _ => wp,
            };
            change(key, msg == 0x100 || msg == 0x104);
            if (0xa0..=0xa5).contains(&key) {
                let left = (key & !1) as usize;
                let generic = (0x10 + (key - 0xa0) / 2) as usize;
                let down = (state[left] | state[left + 1]) & 0x80 != 0;
                let was = state[generic] & 0x80 != 0;
                state[generic] = if down { 0x80 } else { 0 };
                if physical && down && !was {
                    self.pressed[generic] = 1;
                }
            }
        } else if (0x201..=0x209).contains(&msg) {
            let index = msg - 0x201;
            change([1, 2, 4][(index / 3) as usize], index % 3 != 1);
        } else if (0x20b..=0x20d).contains(&msg) {
            change(if wp >> 16 == 2 { 6 } else { 5 }, msg != 0x20c);
        }
    }
    pub fn mouse_flags(&self) -> u32 {
        [
            (1, 1),
            (2, 2),
            (0x10, 4),
            (0x11, 8),
            (4, 16),
            (5, 32),
            (6, 64),
        ]
        .iter()
        .fold(0, |flags, (key, flag)| {
            flags
                | if self.physical[*key] & 0x80 != 0 {
                    *flag
                } else {
                    0
                }
        })
    }
}
