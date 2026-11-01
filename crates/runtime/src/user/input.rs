//! Browser events update owned state and enqueue messages; they never reenter guest code.
use super::*;

pub(super) struct Input {
    pub cursor: [i32; 2],
    buttons: u32,
    pub keyboard: super::keyboard::Keyboard,
    pub cursor_count: i32,
    pub cursor_handle: u32,
}
impl Default for Input {
    fn default() -> Self {
        Self {
            cursor: [400, 300],
            buttons: 0,
            keyboard: super::keyboard::Keyboard::new(),
            cursor_count: 0,
            cursor_handle: 0,
        }
    }
}
impl User {
    pub(super) fn translate_message(&mut self, message: Message) -> Result<u32> {
        if matches!(message.id, 0x100 | 0x104) {
            if let Some(character) = self.input.keyboard.character(message.wparam) {
                self.enqueue(Message {
                    id: if message.id == 0x104 { 0x106 } else { 0x102 },
                    wparam: character,
                    input: false,
                    ..message
                })?;
            }
        }
        Ok(u32::from(matches!(
            message.id,
            0x100 | 0x101 | 0x104 | 0x105
        )))
    }
    pub(crate) fn mouse_input(&mut self, x: i32, y: i32, buttons: u32) -> Result<()> {
        if buttons & !7 != 0 {
            return Err(Error::Host(
                "mouse input supports left, right and middle buttons".into(),
            ));
        }
        let target = self
            .focus
            .or((self.capture != 0).then_some(self.capture))
            .and_then(|hwnd| {
                self.windows
                    .get(&hwnd)
                    .map(|window| (hwnd, window.x, window.y))
            });
        let cursor = target.map_or([x, y], |(_, wx, wy)| {
            [x.wrapping_add(wx as i32), y.wrapping_add(wy as i32)]
        });
        let changed = self.input.buttons ^ buttons;
        let moved = self.input.cursor != cursor;
        let required = changed.count_ones() as usize + usize::from(moved);
        if target.is_some() && self.messages.len() + required > MAX_MESSAGES {
            return Err(Error::Host("window message queue capacity exceeded".into()));
        }
        self.input.cursor = cursor;
        self.input.buttons = buttons;
        for (bit, down) in [(1, 0x201), (2, 0x204), (4, 0x207)] {
            if changed & bit != 0 {
                self.input
                    .keyboard
                    .update(down + u32::from(buttons & bit == 0), 0, 0, true);
            }
        }
        let mask = self.input.keyboard.mouse_flags();
        let packed = (x as u32 & 0xffff) | ((y as u32 & 0xffff) << 16);
        if let Some((hwnd, _, _)) = target {
            if moved {
                self.enqueue_mouse(Message {
                    time: 0,
                    x: cursor[0] as u32,
                    y: cursor[1] as u32,
                    ..Message::new(hwnd, 0x200, mask, packed)
                })?;
            }
            for (bit, _vk, down, up) in [
                (1, 1, 0x201, 0x202),
                (2, 2, 0x204, 0x205),
                (4, 4, 0x207, 0x208),
            ] {
                if changed & bit != 0 {
                    let pressed = buttons & bit != 0;
                    self.enqueue_mouse(Message {
                        time: 0,
                        x: cursor[0] as u32,
                        y: cursor[1] as u32,
                        ..Message::new(hwnd, if pressed { down } else { up }, mask, packed)
                    })?;
                }
            }
        }
        Ok(())
    }
    fn enqueue_mouse(&mut self, mut message: Message) -> Result<()> {
        message.input = true;
        if message.id == 0x200 {
            if let Some(previous) = self.messages.back_mut() {
                if previous.id == message.id && previous.hwnd == message.hwnd {
                    *previous = message;
                    return Ok(());
                }
            }
        }
        self.enqueue(message)
    }
    pub(crate) fn key_input(&mut self, vk: u32, down: bool, repeat: bool) -> Result<()> {
        if vk > 255 {
            return Err(Error::InvalidRange);
        }
        if self.focus.is_some() && self.messages.len() == MAX_MESSAGES {
            return Err(Error::Host("window message queue capacity exceeded".into()));
        }
        let index = vk as usize;
        let previous = self.input.keyboard.physical[index] & 0x80 != 0;
        let alt = self.input.keyboard.physical[0x12] & 0x80 != 0 || vk == 0x12;
        let hwnd = self.focus;
        {
            let id = match (alt, down) {
                (false, true) => 0x100,
                (false, false) => 0x101,
                (true, true) => 0x104,
                (true, false) => 0x105,
            };
            let (scan, extended) = scan_code(vk);
            let lparam = 1
                | (scan << 16)
                | (u32::from(extended) << 24)
                | (u32::from(alt) << 29)
                | (u32::from(previous || repeat || !down) << 30)
                | (u32::from(!down) << 31);
            self.input.keyboard.update(id, vk, lparam, true);
            if let Some(hwnd) = hwnd {
                self.enqueue(Message {
                    input: true,
                    x: self.input.cursor[0] as u32,
                    y: self.input.cursor[1] as u32,
                    ..Message::new(hwnd, id, vk, lparam)
                })?;
            }
        }
        Ok(())
    }
    pub(super) fn key_state(&mut self, vk: u32, asynchronous: bool) -> Result<u32> {
        if vk >= 256 {
            return Ok(0);
        }
        let i = vk as usize;
        let value = if asynchronous {
            let value = ((self.input.keyboard.physical[i] as u16 & 0x80) << 8)
                | self.input.keyboard.pressed[i] as u16;
            self.input.keyboard.pressed[i] = 0;
            value
        } else {
            let state = self.input.keyboard.queued[i] as u16;
            ((state & 0x80) << 8) | (state & 1)
        };
        Ok(value as i16 as i32 as u32)
    }
    pub(super) fn consume_input(&mut self, message: Message) {
        if message.input {
            self.input
                .keyboard
                .update(message.id, message.wparam, message.lparam, false);
        }
    }
    pub(super) fn set_focus(&mut self, hwnd: u32) -> u32 {
        if hwnd != 0 && !self.windows.contains_key(&hwnd) {
            return self.fail(1400);
        }
        let previous = self.focus.unwrap_or(0);
        self.focus = (hwnd != 0).then_some(hwnd);
        previous
    }
}
/// Set-1 keyboard scan codes for the supported browser virtual-key input set.
fn scan_code(vk: u32) -> (u32, bool) {
    let code = match vk {
        0x41..=0x5a => [
            0x1e, 0x30, 0x2e, 0x20, 0x12, 0x21, 0x22, 0x23, 0x17, 0x24, 0x25, 0x26, 0x32, 0x31,
            0x18, 0x19, 0x10, 0x13, 0x1f, 0x14, 0x16, 0x2f, 0x11, 0x2d, 0x15, 0x2c,
        ][(vk - 0x41) as usize],
        0x31..=0x39 => vk - 0x31 + 2,
        0x30 => 11,
        0x08 => 14,
        0x09 => 15,
        0x0d => 28,
        0x10 => 42,
        0x11 => 29,
        0x12 => 56,
        0x14 => 58,
        0x1b => 1,
        0x20 => 57,
        0x21 => 73,
        0x22 => 81,
        0x23 => 79,
        0x24 => 71,
        0x25 => 75,
        0x26 => 72,
        0x27 => 77,
        0x28 => 80,
        0x2d => 82,
        0x2e => 83,
        0x70..=0x79 => 59 + vk - 0x70,
        0x7a => 87,
        0x7b => 88,
        0x90 => 69,
        0x91 => 70,
        0x60..=0x69 => [82, 79, 80, 81, 75, 76, 77, 71, 72, 73][(vk - 0x60) as usize],
        0x6a => 55,
        0x6b => 78,
        0x6d => 74,
        0x6e => 83,
        0x6f => 53,
        0xba => 39,
        0xbb => 13,
        0xbc => 51,
        0xbd => 12,
        0xbe => 52,
        0xbf => 53,
        0xc0 => 41,
        0xdb => 26,
        0xdc => 43,
        0xdd => 27,
        0xde => 40,
        _ => 0,
    };
    (code, matches!(vk, 0x21..=0x28 | 0x2d | 0x2e))
}
