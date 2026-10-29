//! One virtual display per process, shared by USER32 and DirectDraw.
use std::{cell::Cell, rc::Rc};
pub(crate) type Mode = (u32, u32, u32);
pub(crate) const DEFAULT_MODE: Mode = (800, 600, 16);
pub(crate) const MODES: [Mode; 3] = [(640, 480, 16), DEFAULT_MODE, (1024, 768, 16)];
#[derive(Clone)]
pub(crate) struct Display(Rc<Cell<Mode>>);
impl Default for Display {
    fn default() -> Self {
        Self(Rc::new(Cell::new(DEFAULT_MODE)))
    }
}
impl Display {
    pub fn mode(&self) -> Mode {
        self.0.get()
    }
    pub fn set_mode(&self, mode: Mode) {
        self.0.set(mode);
    }
}
