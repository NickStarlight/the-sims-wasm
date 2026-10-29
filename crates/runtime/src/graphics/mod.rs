//! Software DirectDraw: Rust-owned COM metadata and pixels, bulk guest locks and frames.
mod attachments;
mod blit_flags;
mod clipper;
mod com;
mod d3d;
mod dc;
mod device;
mod format;
mod palette;
mod raster;
mod surface;
#[cfg(test)]
#[path = "../../tests/unit/graphics/mod.rs"]
mod tests;
use crate::{
    modules::{ModuleCatalog, Symbol},
    Error, GuestMemory, Process, Result,
};
use std::{collections::BTreeMap, rc::Rc};
const INVALID: u32 = 0x80070057;
const NO_INTERFACE: u32 = 0x80004002;
const NOT_IMPLEMENTED: u32 = 0x80004001;
const BUSY: u32 = 0x887601ae;
const MAX_BYTES: usize = 64 * 1024 * 1024;
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Device(u32),
    Surface,
    Palette,
    Clipper,
    D3d(d3d::Interface),
}
struct Object {
    references: u32,
    kind: Kind,
}
#[derive(Clone)]
struct Surface {
    width: u32,
    height: u32,
    pitch: u32,
    format: format::Format,
    caps: u32,
    primary: bool,
    // Runtime writes go through pixels_mut/replace_pixels to invalidate the guest copy.
    pixels: Rc<Vec<u8>>,
    lock: Option<bool>,
    guest_pixels: u32,
    guest_pixels_current: bool,
    dc: u32,
    palette: u32,
    clipper: u32,
    back: u32,
    front: u32,
    depth: u32,
    pixel_format: Option<[u8; 32]>,
    source_key: Option<(u32, u32)>,
    destination_key: Option<(u32, u32)>,
    source_overlay_key: Option<(u32, u32)>,
    destination_overlay_key: Option<(u32, u32)>,
}
impl Surface {
    fn pixels_mut(&mut self) -> &mut Vec<u8> {
        self.guest_pixels_current = false;
        Rc::make_mut(&mut self.pixels)
    }

    fn replace_pixels(&mut self, pixels: Rc<Vec<u8>>) -> Rc<Vec<u8>> {
        self.guest_pixels_current = false;
        std::mem::replace(&mut self.pixels, pixels)
    }
}

struct Palette {
    flags: u32,
    count: usize,
    entries: [[u8; 4]; 256],
}
/// One completed frame; the host owns it after take_frame and can discard older frames.
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
/// Completed image without RGB565 expansion; format is 16 (little-endian RGB565) or 32 (RGBA8).
pub struct PackedFrame {
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub format: u32,
    pub pixels: Rc<Vec<u8>>,
}
// Preserve the completed image and palette even if the surface is subsequently
// changed or released. Pixel conversion happens only when the host takes it.
struct PendingFrame {
    surface: Surface,
    palette: Option<[[u8; 4]; 256]>,
    pixels: Vec<u8>,
}
#[derive(Default)]
pub(crate) struct Graphics {
    d3d: d3d::State,
    objects: BTreeMap<u32, Object>,
    vtables: BTreeMap<String, u32>,
    devices: BTreeMap<u32, u32>,
    surfaces: BTreeMap<u32, Surface>,
    palettes: BTreeMap<u32, Palette>,
    clippers: BTreeMap<u32, clipper::Clipper>,
    display: crate::display::Display,
    cooperative: u32,
    frame: Option<PendingFrame>,
    // One retired, uniquely owned image can receive a subsequent full Unlock.
    // Never recycle a buffer still referenced by a surface or a returned frame.
    recycled_pixels: Option<Vec<u8>>,
    enumeration_buffers: Vec<u32>,
}
impl Graphics {
    pub(crate) fn with_display(display: crate::display::Display) -> Self {
        Self {
            display,
            ..Self::default()
        }
    }
    pub fn take_frame(&mut self) -> Option<Frame> {
        let PendingFrame {
            surface,
            palette,
            mut pixels,
        } = self.frame.take()?;
        let size = surface.width as usize * surface.height as usize * 4;
        if surface.format == format::Format::new(16).unwrap() {
            let colors = &*format::RGB565;
            pixels.resize(size, 0);
            let width = surface.width as usize;
            for y in 0..surface.height as usize {
                let start = y * surface.pitch as usize;
                let source = &surface.pixels[start..start + width * 2];
                let destination = &mut pixels[y * width * 4..(y + 1) * width * 4];
                for (dst, src) in destination
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(source.as_chunks::<2>().0)
                {
                    *dst = colors[usize::from(u16::from_le_bytes(*src))];
                }
            }
        } else {
            for y in 0..surface.height {
                for x in 0..surface.width {
                    let value = format::read_pixel(&surface, x, y);
                    pixels.extend(surface.format.rgba(value, palette.as_ref()));
                }
            }
        }
        Some(Frame {
            width: surface.width,
            height: surface.height,
            pixels,
        })
    }
    pub fn take_packed_frame(&mut self) -> Option<PackedFrame> {
        if self.frame.as_ref()?.surface.format == format::Format::new(16).unwrap() {
            let surface = self.frame.take()?.surface;
            Some(PackedFrame {
                width: surface.width,
                height: surface.height,
                pitch: surface.pitch,
                format: 16,
                pixels: surface.pixels,
            })
        } else {
            self.take_frame().map(|frame| PackedFrame {
                width: frame.width,
                height: frame.height,
                pitch: frame.width * 4,
                format: 32,
                pixels: Rc::new(frame.pixels),
            })
        }
    }
    pub fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        if name == "Surface_GetDDInterface" {
            if !self.surfaces.contains_key(&args[0]) {
                return Ok(INVALID);
            }
            if args[1] != 0 {
                let device = self.device(process, modules, 1)?;
                write_u32(process, args[1], device)?;
            }
            return Ok(0);
        }
        if name.ends_with("QueryInterface") {
            return self.query(process, modules, args);
        }
        if name.ends_with("AddRef") {
            return Ok(self.add_ref(args[0]));
        }
        if name.ends_with("Release") {
            return self.release(process, args[0]);
        }
        if name.starts_with("D3D") {
            return self.d3d_call(process, modules, name, args);
        }
        if name.starts_with("Clipper_") {
            return self.clipper_call(process, name, args);
        }
        if name.starts_with("Palette_") {
            return self.palette_call(process, name, args);
        }
        if name.starts_with("Surface_") {
            return self.surface_call(process, name, args);
        }
        self.device_call(process, modules, name, args)
    }
    fn present(&mut self, id: u32) -> Result<()> {
        let Some(surface) = self.surfaces.get(&id) else {
            return Ok(());
        };
        if !surface.primary {
            return Ok(());
        }
        let palette = self.palettes.get(&surface.palette).map(|p| p.entries);
        let size = (surface.width as usize)
            .checked_mul(surface.height as usize)
            .and_then(|n| n.checked_mul(4))
            .filter(|&n| n <= MAX_BYTES)
            .ok_or(Error::OutOfMemory)?;
        // A backpressured host leaves the latest frame here. Reuse only that
        // unconsumed buffer; previously returned frames retain independent ownership.
        let pixels =
            if let Some(frame) = self.frame.as_mut().filter(|f| f.pixels.capacity() >= size) {
                std::mem::take(&mut frame.pixels)
            } else {
                let mut pixels = Vec::new();
                pixels
                    .try_reserve_exact(size)
                    .map_err(|_| Error::OutOfMemory)?;
                pixels
            };
        let previous = self.frame.replace(PendingFrame {
            surface: surface.clone(),
            palette,
            pixels,
        });
        if let Some(previous) = previous {
            if self.recycled_pixels.is_none() {
                self.recycled_pixels = Rc::try_unwrap(previous.surface.pixels).ok();
            }
        }
        Ok(())
    }
}
fn read_u32<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<u32> {
    let mut b = [0; 4];
    process.read(address, &mut b)?;
    Ok(u32::from_le_bytes(b))
}
fn write_u32<M: GuestMemory>(process: &mut Process<M>, address: u32, value: u32) -> Result<()> {
    process.write(address, &value.to_le_bytes())
}
fn zero_bytes(size: usize) -> Result<Vec<u8>> {
    if size > MAX_BYTES {
        return Err(Error::OutOfMemory);
    }
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size)
        .map_err(|_| Error::OutOfMemory)?;
    bytes.resize(size, 0);
    Ok(bytes)
}
