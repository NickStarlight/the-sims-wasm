use super::*;

pub(super) struct Window {
    pub class_atom: u32,
    pub class_name: Vec<u8>,
    pub title: Vec<u8>,
    pub parent: u32,
    pub procedure: u32,
    pub instance: u32,
    pub owner: u32,
    pub style: u32,
    pub ex_style: u32,
    pub user_data: u32,
    pub extra: Vec<u8>,
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
    pub visible: bool,
    slot: u32,
    destroying: bool,
}

pub(crate) enum Continuation {
    DestroyCaptured {
        hwnd: u32,
    },
    Notifications {
        hwnd: u32,
        messages: Vec<(u32, u32, u32)>,
    },
    Position {
        address: u32,
        stage: super::position::Stage,
    },
    NcCreate {
        hwnd: u32,
        structure: u32,
    },
    Create {
        hwnd: u32,
    },
    Destroy {
        hwnd: u32,
        value: u32,
    },
    NcDestroy {
        hwnd: u32,
        value: u32,
    },
    Result,
    Constant(u32),
}
pub(crate) enum Action {
    Return(u32),
    Invoke {
        address: u32,
        args: Vec<u32>,
        next: Continuation,
    },
}
impl User {
    pub(crate) fn begin<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<Action> {
        match name {
            "CallWindowProcA" => {
                if a[0] == 0 {
                    return Ok(Action::Return(self.fail(87)));
                }
                Ok(Action::Invoke {
                    address: a[0],
                    args: a[1..5].to_vec(),
                    next: Continuation::Result,
                })
            }
            "DefWindowProcA" => self.default_action(process, a),
            "SetWindowPos" => self.set_position(process, a),
            "CreateWindowExA" => self.create(process, a),
            "DestroyWindow" => {
                if self.capture == a[0] && self.capture != 0 {
                    self.capture = 0;
                    self.invoke(
                        a[0],
                        0x215,
                        0,
                        0,
                        Continuation::DestroyCaptured { hwnd: a[0] },
                    )
                } else {
                    self.destroy(a[0], 1)
                }
            }
            "SetCapture" => self.change_capture(a[0], None),
            "ReleaseCapture" => self.change_capture(0, Some(1)),
            "SendMessageA" => self.invoke(a[0], a[1], a[2], a[3], Continuation::Result),
            "DispatchMessageA" => {
                let m = Message::read(process, a[0])?;
                if m.id == 0x113 && m.lparam != 0 {
                    return Ok(Action::Invoke {
                        address: m.lparam,
                        args: vec![m.hwnd, m.id, m.wparam, m.time],
                        next: Continuation::Constant(0),
                    });
                }
                self.invoke(m.hwnd, m.id, m.wparam, m.lparam, Continuation::Result)
            }
            _ => Err(Error::Host(format!(
                "unsupported callback USER32 service {name}"
            ))),
        }
    }
    fn create<M: GuestMemory>(&mut self, process: &mut Process<M>, a: &[u32]) -> Result<Action> {
        let Some(class) = self.class(process, a[1], a[10])? else {
            return Ok(Action::Return(self.fail(1407)));
        };
        let procedure = class.procedure;
        let extra = vec![0; class.window_extra];
        let class_atom = class.atom;
        let class_name = class.name.clone();
        let title = if a[2] == 0 {
            Vec::new()
        } else {
            process.read_c_string(a[2], 65536)?
        };
        // For non-child windows hWndParent denotes an owner, not a hierarchy parent.
        let parent = if a[3] & 0x4000_0000 != 0 { a[8] } else { 0 };
        if self.windows.len() >= MAX_WINDOWS {
            return Ok(Action::Return(self.fail(8)));
        }
        if a[8] != 0 && !self.windows.contains_key(&a[8]) {
            return Ok(Action::Return(self.fail(1400)));
        }
        let slot = (0..MAX_WINDOWS as u32)
            .find(|slot| self.windows.values().all(|w| w.slot != *slot))
            .ok_or(Error::OutOfMemory)?;
        let base = match self.structures {
            Some(base) => base,
            None => {
                let base = process.heap_alloc(MAX_WINDOWS as u32 * 48)?;
                self.structures = Some(base);
                base
            }
        };
        let structure = base + slot * 48;
        let width = dimension(a[6], self.display.mode().0);
        let height = dimension(a[7], self.display.mode().1);
        let x = coordinate(a[4]);
        let y = coordinate(a[5]);
        write_words(
            process,
            structure,
            &[
                a[11], a[10], a[9], a[8], height, width, y, x, a[3], a[2], a[1], a[0],
            ],
        )?;
        let hwnd = 0x0760_0000u32
            .checked_add(
                self.next_window
                    .checked_add(1)
                    .ok_or(Error::OutOfMemory)?
                    .checked_mul(4)
                    .ok_or(Error::OutOfMemory)?,
            )
            .ok_or(Error::OutOfMemory)?;
        self.next_window += 1;
        self.windows.insert(
            hwnd,
            Window {
                class_atom,
                class_name,
                title,
                parent,
                procedure,
                instance: a[10],
                owner: if parent == 0 { a[8] } else { 0 },
                style: a[3],
                ex_style: a[0],
                user_data: 0,
                extra,
                x,
                y,
                width,
                height,
                visible: a[3] & 0x1000_0000 != 0,
                slot,
                destroying: false,
            },
        );
        self.mark_dirty(hwnd)?;
        self.invoke(
            hwnd,
            0x81,
            0,
            structure,
            Continuation::NcCreate { hwnd, structure },
        )
    }
    pub(super) fn invoke(
        &mut self,
        hwnd: u32,
        id: u32,
        wparam: u32,
        lparam: u32,
        next: Continuation,
    ) -> Result<Action> {
        let Some(window) = self.windows.get(&hwnd) else {
            return Ok(Action::Return(0));
        };
        Ok(Action::Invoke {
            address: window.procedure,
            args: vec![hwnd, id, wparam, lparam],
            next,
        })
    }
    fn destroy(&mut self, hwnd: u32, value: u32) -> Result<Action> {
        let Some(window) = self.windows.get_mut(&hwnd) else {
            return Ok(Action::Return(self.fail(1400)));
        };
        if window.destroying {
            return Ok(Action::Return(0));
        }
        window.destroying = true;
        self.invoke(hwnd, 2, 0, 0, Continuation::Destroy { hwnd, value })
    }
    pub(crate) fn resume(&mut self, next: Continuation, result: u32) -> Result<Action> {
        match next {
            Continuation::DestroyCaptured { hwnd } => self.destroy(hwnd, 1),
            Continuation::Notifications { hwnd, messages } => self.notify(hwnd, messages),
            Continuation::Position { .. } => Err(Error::Host(
                "window position continuation requires guest memory".into(),
            )),
            Continuation::NcCreate { hwnd, structure } => {
                if result == 0 {
                    return self.invoke(
                        hwnd,
                        0x82,
                        0,
                        0,
                        Continuation::NcDestroy { hwnd, value: 0 },
                    );
                }
                self.invoke(hwnd, 1, 0, structure, Continuation::Create { hwnd })
            }
            Continuation::Create { hwnd } => {
                if result == u32::MAX {
                    return self.destroy(hwnd, 0);
                }
                if let Some(window) = self.windows.get(&hwnd) {
                    if window.visible {
                        self.focus = Some(hwnd);
                    }
                    Ok(Action::Return(hwnd))
                } else {
                    Ok(Action::Return(0))
                }
            }
            Continuation::Destroy { hwnd, value } => {
                self.invoke(hwnd, 0x82, 0, 0, Continuation::NcDestroy { hwnd, value })
            }
            Continuation::NcDestroy { hwnd, value } => {
                self.remove(hwnd);
                Ok(Action::Return(value))
            }
            Continuation::Result => Ok(Action::Return(result)),
            Continuation::Constant(value) => Ok(Action::Return(value)),
        }
    }
    pub(crate) fn resume_with_memory<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        next: Continuation,
        result: u32,
    ) -> Result<Action> {
        match next {
            Continuation::Position { address, stage } => {
                self.resume_position(process, address, stage)
            }
            next => self.resume(next, result),
        }
    }
    pub(crate) fn cancel(&mut self, next: Continuation) {
        match next {
            Continuation::Position { address, .. } => self.cancel_position(address),
            Continuation::DestroyCaptured { hwnd }
            | Continuation::NcCreate { hwnd, .. }
            | Continuation::Create { hwnd }
            | Continuation::Destroy { hwnd, .. }
            | Continuation::NcDestroy { hwnd, .. } => self.remove(hwnd),
            Continuation::Notifications { .. }
            | Continuation::Result
            | Continuation::Constant(_) => {}
        }
    }
    fn remove(&mut self, hwnd: u32) {
        self.windows.remove(&hwnd);
        self.dirty.retain(|window| *window != hwnd);
        if self.capture == hwnd {
            self.capture = 0;
        }
        self.gdi.remove_window(hwnd);
        if self.focus == Some(hwnd) {
            self.focus = None;
        }
        self.messages.retain(|m| m.hwnd != hwnd);
    }
    pub(super) fn show(&mut self, hwnd: u32, command: u32) -> Result<u32> {
        let Some(window) = self.windows.get_mut(&hwnd) else {
            return Ok(self.fail(1400));
        };
        let previous = window.visible;
        window.visible = command != 0;
        if window.visible {
            window.style |= 0x1000_0000;
        } else {
            window.style &= !0x1000_0000;
        }
        if window.visible {
            self.focus = Some(hwnd);
        } else if self.focus == Some(hwnd) {
            self.focus = None;
        }
        if previous != window.visible {
            self.enqueue(Message::new(hwnd, 0x18, u32::from(command != 0), 0))?;
        }
        Ok(u32::from(previous))
    }
}
fn dimension(value: u32, default: u32) -> u32 {
    if value == 0x8000_0000 {
        default
    } else {
        value
    }
}
fn coordinate(value: u32) -> u32 {
    if value == 0x8000_0000 {
        0
    } else {
        value
    }
}
