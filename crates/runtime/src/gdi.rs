//! Owned display DC leases. Drawing objects and raster operations are separate services.
mod bitmap;
mod compatibility;
mod objects;
mod palette;
mod state;
use crate::display::Mode;
pub(crate) use state::ColorAttribute;
use std::collections::BTreeMap;
const MAX_CONTEXTS: usize = 4096;
struct Context {
    compatible: bool,
    selected: u32,
    surface: Option<u32>,
    window: u32,
    thread: u32,
    map_mode: u32,
    palette: u32,
    palette_background: bool,
    background_mode: state::BackgroundMode,
    background_color: state::ColorRef,
    text_color: state::ColorRef,
}
#[derive(Default)]
pub(crate) struct Gdi {
    contexts: BTreeMap<u32, Context>,
    objects: BTreeMap<u32, objects::Object>,
    serial: u32,
    object_serial: u32,
}
impl Gdi {
    pub fn acquire(&mut self, window: u32, thread: u32) -> std::result::Result<u32, u32> {
        if self.contexts.len() >= MAX_CONTEXTS {
            return Err(8);
        }
        let serial = self
            .serial
            .checked_add(1)
            .filter(|&id| id < 0x0100_0000)
            .ok_or(8u32)?;
        let handle = 0x0d00_0000 | serial;
        self.stock_object(15)?;
        self.serial = serial;
        self.contexts.insert(
            handle,
            Context {
                compatible: false,
                selected: 0,
                surface: None,
                window,
                thread,
                map_mode: 1,
                palette: palette::DEFAULT_PALETTE,
                palette_background: false,
                background_mode: state::BackgroundMode::Opaque,
                background_color: state::ColorRef::WHITE,
                text_color: state::ColorRef::BLACK,
            },
        );
        Ok(handle)
    }
    pub fn acquire_surface(&mut self, surface: u32, thread: u32) -> std::result::Result<u32, u32> {
        let handle = self.acquire(0, thread)?;
        self.contexts.get_mut(&handle).unwrap().surface = Some(surface);
        Ok(handle)
    }
    pub fn release_surface(&mut self, surface: u32, handle: u32, thread: u32) -> bool {
        if !self
            .contexts
            .get(&handle)
            .is_some_and(|dc| dc.surface == Some(surface) && dc.thread == thread)
        {
            return false;
        }
        self.contexts.remove(&handle);
        true
    }
    pub fn set_map_mode(&mut self, handle: u32, mode: u32) -> std::result::Result<u32, u32> {
        let context = self.contexts.get_mut(&handle).ok_or(6u32)?;
        if !(1..=8).contains(&mode) {
            return Err(87);
        }
        // MM_TEXT is the only coordinate system currently implemented by this raster domain.
        if mode != 1 {
            return Err(50);
        }
        Ok(std::mem::replace(&mut context.map_mode, mode))
    }
    pub fn map_mode(&self, handle: u32) -> std::result::Result<u32, u32> {
        self.contexts
            .get(&handle)
            .map(|context| context.map_mode)
            .ok_or(6)
    }
    pub fn release(&mut self, window: u32, handle: u32, thread: u32) -> bool {
        if !self
            .contexts
            .get(&handle)
            .is_some_and(|dc| dc.surface.is_none() && dc.window == window && dc.thread == thread)
        {
            return false;
        }
        self.contexts.remove(&handle);
        true
    }
    pub fn remove_window(&mut self, window: u32) {
        self.contexts.retain(|_, dc| dc.window != window);
    }
    pub fn caps(&self, handle: u32, index: u32, mode: Mode) -> std::result::Result<u32, u32> {
        if !self.contexts.contains_key(&handle) {
            return Err(6);
        }
        let (width, height, bits) = mode;
        Ok(match index {
            0 => 0x400,                           // DRIVERVERSION
            2 => 1,                               // DT_RASDISPLAY
            4 => width.saturating_mul(254) / 960, // virtual 96 DPI, millimeters
            6 => height.saturating_mul(254) / 960,
            8 | 118 => width,   // HORZRES / DESKTOPHORZRES
            10 | 117 => height, // VERTRES / DESKTOPVERTRES
            12 => bits,         // BITSPIXEL
            14 => 1,            // PLANES
            24 => {
                if bits <= 8 {
                    1 << bits
                } else {
                    u32::MAX
                }
            } // NUMCOLORS
            88 | 90 => 96,      // LOGPIXELSX/Y
            104 => {
                if bits <= 8 {
                    1 << bits
                } else {
                    0
                }
            } // SIZEPALETTE
            106 => {
                if bits <= 8 {
                    20
                } else {
                    0
                }
            } // NUMRESERVED
            108 => {
                if bits == 32 {
                    24
                } else {
                    bits
                }
            } // COLORRES (padding is not a color channel)
            116 => 60,          // VREFRESH
            _ => 0, // No GDI drawing capabilities advertised before those services exist.
        })
    }
}
