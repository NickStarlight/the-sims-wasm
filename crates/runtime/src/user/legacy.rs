//! Preserved USER32 compatibility behavior from the working runtime; see PORT.md.
use super::*;
impl User {
    pub(crate) fn set_time(&mut self, time: u32) {
        self.time = time;
    }
    pub(super) fn legacy_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        match name {
            "GetKeyboardState" => {
                if a[0] == 0 {
                    return Ok(0);
                }
                process.write(a[0], &self.input.keyboard.queued)?;
                Ok(1)
            }
            "ShowCursor" => {
                self.input.cursor_count =
                    self.input
                        .cursor_count
                        .wrapping_add(if a[0] != 0 { 1 } else { -1 });
                Ok(self.input.cursor_count as u32)
            }
            "SetCursor" => Ok(std::mem::replace(&mut self.input.cursor_handle, a[0])),
            "SetCursorPos" => {
                self.input.cursor = [a[0] as i32, a[1] as i32];
                Ok(1)
            }
            "GetCapture" => Ok(self.capture),
            "IsWindowVisible" => Ok(u32::from(
                self.windows.get(&a[0]).is_some_and(|w| w.visible),
            )),
            "InvalidateRect" => {
                if a[0] != 0 {
                    self.mark_dirty(a[0])?;
                } else {
                    for hwnd in self.windows.keys().copied().collect::<Vec<_>>() {
                        self.mark_dirty(hwnd)?;
                    }
                }
                Ok(1)
            }
            "UpdateWindow" => {
                self.mark_dirty(a[0])?;
                Ok(1)
            }
            "DrawTextA" => super::text_measure::draw_text(process, a),
            _ => Err(Error::Host(format!(
                "unknown migrated USER32 service {name}"
            ))),
        }
    }
    pub(super) fn mark_dirty(&mut self, hwnd: u32) -> Result<()> {
        if !self.dirty.contains(&hwnd) {
            if self.dirty.len() >= MAX_MESSAGES {
                return Err(Error::Host("dirty-window queue capacity exceeded".into()));
            }
            self.dirty.push_back(hwnd);
        }
        Ok(())
    }
    pub(super) fn refill(&mut self) -> Result<()> {
        if !self.messages.is_empty() {
            return Ok(());
        }
        let message = if let Some(hwnd) = self.dirty.pop_front() {
            Some(Message::new(hwnd, 0xf, 0, 0))
        } else if self.time.wrapping_sub(self.last_tick) >= 15 {
            self.last_tick = self.time;
            Some(Message::new(
                self.windows.keys().next().copied().unwrap_or(0),
                0x113,
                0,
                0,
            ))
        } else {
            None
        };
        if let Some(mut message) = message {
            message.time = self.time;
            self.enqueue(message)?;
        }
        Ok(())
    }
    pub(super) fn destination(&self, message: &Message) -> u32 {
        if message.input && (0x200..=0x20e).contains(&message.id) && self.capture != 0 {
            self.capture
        } else {
            message.hwnd
        }
    }
    pub(super) fn change_capture(&mut self, hwnd: u32, value: Option<u32>) -> Result<Action> {
        if hwnd != 0 && !self.windows.contains_key(&hwnd) {
            return Ok(Action::Return(0));
        }
        let previous = std::mem::replace(&mut self.capture, hwnd);
        let result = value.unwrap_or(previous);
        if previous != 0 && previous != hwnd && self.windows.contains_key(&previous) {
            self.invoke(previous, 0x215, 0, hwnd, Continuation::Constant(result))
        } else {
            Ok(Action::Return(result))
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/user/legacy.rs"]
mod tests;
