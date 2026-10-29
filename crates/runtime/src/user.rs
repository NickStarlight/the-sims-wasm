//! Single-thread, borderless desktop windows and message queues owned by one process.
mod classes;
mod compatibility;
mod dc;
mod display;
mod geometry;
mod input;
mod keyboard;
mod legacy;
mod messages;
mod position;
mod properties;
mod rectangles;
mod search;
#[cfg(test)]
#[path = "../tests/unit/user.rs"]
mod tests;
mod text_measure;
mod windows;

use crate::{Error, GuestMemory, Process, Result};
use classes::Class;
use messages::Message;
use std::collections::{BTreeMap, VecDeque};
use windows::Window;
pub(crate) use windows::{Action, Continuation};

const MAX_WINDOWS: usize = 1024;
const MAX_MESSAGES: usize = 4096;

#[derive(Default)]
pub(crate) struct User {
    gdi: crate::gdi::Gdi,
    display: crate::display::Display,
    classes: BTreeMap<u32, Class>,
    windows: BTreeMap<u32, Window>,
    messages: VecDeque<Message>,
    next_atom: u32,
    next_window: u32,
    structures: Option<u32>,
    error: Option<u32>,
    input: input::Input,
    focus: Option<u32>,
    capture: u32,
    dirty: VecDeque<u32>,
    time: u32,
    last_tick: u32,
    positions: position::Positions,
}
impl User {
    pub(crate) fn with_display(display: crate::display::Display) -> Self {
        Self {
            display,
            ..Self::default()
        }
    }
    pub(crate) fn take_error(&mut self) -> Option<u32> {
        self.error.take()
    }
    fn fail(&mut self, error: u32) -> u32 {
        self.error = Some(error);
        0
    }

    pub(crate) fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        match name {
            "SetWindowTextA" => Ok(1), // Original shim only logged the supplied text.
            "SetRect" | "OffsetRect" | "InflateRect" | "IntersectRect" | "UnionRect"
            | "IsRectEmpty" | "PtInRect" => rectangles::call(process, name, a),
            "GetKeyboardState" | "ShowCursor" | "SetCursor" | "SetCursorPos" | "GetCapture"
            | "InvalidateRect" | "UpdateWindow" | "IsWindowVisible" | "DrawTextA" => {
                self.legacy_call(process, name, a)
            }
            "EnumDisplaySettingsA" => self.enum_display_settings(process, a),
            "FindWindowA" => self.find_window(process, &[0, 0, a[0], a[1]]),
            "FindWindowExA" => self.find_window(process, a),
            "RegisterClassA" | "RegisterClassExA" => {
                self.register(process, a[0], name.ends_with("ExA"))
            }
            "GetClassInfoA" | "GetClassInfoExA" => {
                self.class_info(process, a, name.ends_with("ExA"))
            }
            "GetWindowLongA" => Ok(self.window_long(a[0], a[1] as i32, None)),
            "SetWindowLongA" => Ok(self.window_long(a[0], a[1] as i32, Some(a[2]))),
            "DefWindowProcA" => self.default_proc(process, a),
            "ShowWindow" => self.show(a[0], a[1]),
            "GetSystemMetrics" => Ok(match a[0] {
                0 | 16 | 59 => self.display.mode().0,
                1 | 17 | 60 => self.display.mode().1,
                19 => 1,
                23 | 43 => 3,
                _ => 0,
            }),
            "AdjustWindowRect" | "AdjustWindowRectEx" => {
                self.adjust_rect(process, a, name.ends_with("Ex"))
            }
            "GetClientRect" | "GetWindowRect" => {
                self.rect(process, a[0], a[1], name == "GetClientRect")
            }
            "ClientToScreen" | "ScreenToClient" => {
                self.coordinate(process, a[0], a[1], name == "ClientToScreen")
            }
            "PeekMessageA" => self.peek(process, a),
            "GetMessageA" => {
                if a[0] == 0 || (a[1] != 0 && a[1] != u32::MAX && !self.windows.contains_key(&a[1]))
                {
                    self.fail(if a[0] == 0 { 87 } else { 1400 });
                    return Ok(u32::MAX);
                }
                let found = self.peek(process, &[a[0], a[1], a[2], a[3], 1])?;
                if found == 0 {
                    // Preserved compatibility fallback: this is not a blocking wait.
                    write_words(process, a[0], &[0, 0, 0, 0, self.time, 0, 0])?;
                    return Ok(1);
                }
                let msg = Message::read(process, a[0])?;
                Ok(u32::from(msg.id != 0x12))
            }
            "TranslateMessage" => {
                let msg = Message::read(process, a[0])?;
                self.translate_message(msg)
            }
            "PostMessageA" => self.post(Message::new(a[0], a[1], a[2], a[3])),
            "PostQuitMessage" => {
                self.enqueue(Message::new(0, 0x12, a[0], 0))?;
                Ok(0)
            }
            "GetCursorPos" => {
                write_words(process, a[0], &self.input.cursor.map(|v| v as u32))?;
                Ok(1)
            }
            "GetKeyState" => self.key_state(a[0], false),
            "GetAsyncKeyState" => self.key_state(a[0], true),
            "GetFocus" => Ok(self.focus.unwrap_or(0)),
            "SetFocus" => Ok(self.set_focus(a[0])),
            "IsWindow" => Ok(u32::from(self.windows.contains_key(&a[0]))),
            _ => compatibility::call(process, name, a),
        }
    }
}
fn words<M: GuestMemory, const N: usize>(process: &Process<M>, address: u32) -> Result<[u32; N]> {
    let mut bytes = [[0; 4]; N];
    process.read(address, bytes.as_flattened_mut())?;
    Ok(bytes.map(u32::from_le_bytes))
}
fn write_words<M: GuestMemory, const N: usize>(
    process: &mut Process<M>,
    address: u32,
    values: &[u32; N],
) -> Result<()> {
    let bytes = values.map(u32::to_le_bytes);
    process.write(address, bytes.as_flattened())
}
