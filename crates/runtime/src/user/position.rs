//! Reentrant SetWindowPos transactions retain private guest structures across callbacks.
use super::*;

const NOSIZE: u32 = 1;
const NOMOVE: u32 = 2;
const NOZORDER: u32 = 4;
const NOACTIVATE: u32 = 0x10;
const FRAMECHANGED: u32 = 0x20;
const NOSENDCHANGING: u32 = 0x400;
const IMMUTABLE_FLAGS: u32 = NOACTIVATE | 0x200;

#[derive(Default)]
pub(super) struct Positions {
    active: BTreeMap<u32, Position>,
    free: Vec<u32>,
}
#[derive(Clone, Copy)]
struct Position {
    hwnd: u32,
    flags: u32,
    old: [u32; 4],
    prepared: [u32; 7],
}
#[derive(Clone, Copy)]
pub(crate) enum Stage {
    Changing,
    Calculated,
    Changed,
}

impl User {
    pub(super) fn set_position<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<Action> {
        let Some(window) = self.windows.get(&args[0]) else {
            return Ok(Action::Return(self.fail(1400)));
        };
        let old = rectangle(window.x, window.y, window.width, window.height)?;
        let fields = [
            args[0], args[1], args[2], args[3], args[4], args[5], args[6],
        ];
        if let Some(error) = validate_flags(fields[6]) {
            return Ok(Action::Return(self.fail(error)));
        }
        if self.positions.active.len() >= 64 {
            return Ok(Action::Return(self.fail(8)));
        }
        let address = match self.positions.free.pop() {
            Some(address) => address,
            None => process.heap_alloc(96)?,
        };
        if let Err(error) = write_words(process, address, &fields) {
            self.positions.free.push(address);
            return Err(error);
        }
        self.positions.active.insert(
            address,
            Position {
                hwnd: args[0],
                flags: args[6],
                old,
                prepared: fields,
            },
        );
        if args[6] & NOSENDCHANGING != 0 {
            self.resume_position(process, address, Stage::Changing)
        } else {
            self.invoke(
                args[0],
                0x46,
                0,
                address,
                Continuation::Position {
                    address,
                    stage: Stage::Changing,
                },
            )
        }
    }
    pub(super) fn resume_position<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        address: u32,
        stage: Stage,
    ) -> Result<Action> {
        let result = self.advance_position(process, address, stage);
        if result.is_err() {
            self.cancel_position(address);
        }
        result
    }
    fn advance_position<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        address: u32,
        stage: Stage,
    ) -> Result<Action> {
        let state = *self
            .positions
            .active
            .get(&address)
            .ok_or_else(|| Error::Host("missing window-position transaction".into()))?;
        if matches!(stage, Stage::Changed) {
            self.cancel_position(address);
            return Ok(Action::Return(1));
        }
        let Some(window) = self.windows.get(&state.hwnd) else {
            return Ok(self.position_failure(address, 1400));
        };
        match stage {
            Stage::Changing => {
                let mut fields = words::<_, 7>(process, address)?;
                if fields[0] != state.hwnd {
                    return Ok(self.position_failure(address, 87));
                }
                fields[6] = (fields[6] & !IMMUTABLE_FLAGS) | (state.flags & IMMUTABLE_FLAGS);
                if let Some(error) = validate_flags(fields[6]) {
                    return Ok(self.position_failure(address, error));
                }
                if fields[6] & NOMOVE != 0 {
                    fields[2] = window.x;
                    fields[3] = window.y;
                }
                if fields[6] & NOSIZE != 0 {
                    fields[4] = window.width;
                    fields[5] = window.height;
                }
                let rect = rectangle(fields[2], fields[3], fields[4], fields[5])?;
                write_words(process, address, &fields)?;
                self.positions.active.get_mut(&address).unwrap().prepared = fields;
                if fields[6] & NOSIZE == 0 || fields[6] & FRAMECHANGED != 0 {
                    let mut params = [0; 13];
                    params[..4].copy_from_slice(&rect);
                    params[4..8].copy_from_slice(&state.old);
                    params[8..12].copy_from_slice(&state.old);
                    params[12] = address;
                    write_words(process, address + 32, &params)?;
                    self.invoke(
                        state.hwnd,
                        0x83,
                        1,
                        address + 32,
                        Continuation::Position {
                            address,
                            stage: Stage::Calculated,
                        },
                    )
                } else {
                    self.commit_position(process, address)
                }
            }
            Stage::Calculated => {
                let f = state.prepared;
                if words::<_, 4>(process, address + 32)? != rectangle(f[2], f[3], f[4], f[5])? {
                    return Ok(self.position_failure(address, 120)); // Custom nonclient borders need a separate model.
                }
                self.commit_position(process, address)
            }
            Stage::Changed => unreachable!(),
        }
    }
    fn commit_position<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        address: u32,
    ) -> Result<Action> {
        let fields = self.positions.active[&address].prepared;
        write_words(process, address, &fields)?;
        let window = self.windows.get_mut(&fields[0]).unwrap();
        window.x = fields[2];
        window.y = fields[3];
        window.width = fields[4];
        window.height = fields[5];
        if fields[6] & 0x40 != 0 {
            window.visible = true;
            window.style |= 0x1000_0000;
        }
        if fields[6] & 0x80 != 0 {
            window.visible = false;
            window.style &= !0x1000_0000;
        }
        if !window.visible && self.focus == Some(fields[0]) {
            self.focus = None;
        } else if window.visible && fields[6] & NOACTIVATE == 0 {
            self.focus = Some(fields[0]);
        }
        self.invoke(
            fields[0],
            0x47,
            0,
            address,
            Continuation::Position {
                address,
                stage: Stage::Changed,
            },
        )
    }
    fn position_failure(&mut self, address: u32, error: u32) -> Action {
        self.cancel_position(address);
        Action::Return(self.fail(error))
    }
    pub(super) fn cancel_position(&mut self, address: u32) {
        if self.positions.active.remove(&address).is_some() {
            self.positions.free.push(address);
        }
    }
}
fn validate_flags(flags: u32) -> Option<u32> {
    if flags & !0x7ff != 0 || flags & NOZORDER == 0 {
        Some(120)
    } else if flags & 0xc0 == 0xc0 {
        Some(87)
    } else {
        None
    }
}
fn rectangle(x: u32, y: u32, width: u32, height: u32) -> Result<[u32; 4]> {
    let width = i32::try_from(width).map_err(|_| Error::InvalidRange)?;
    let height = i32::try_from(height).map_err(|_| Error::InvalidRange)?;
    let right = (x as i32).checked_add(width).ok_or(Error::InvalidRange)?;
    let bottom = (y as i32).checked_add(height).ok_or(Error::InvalidRange)?;
    Ok([x, y, right as u32, bottom as u32])
}

#[cfg(test)]
#[path = "../../tests/unit/user/position.rs"]
mod tests;
