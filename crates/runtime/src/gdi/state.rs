//! Logical text/background attributes retained by each DC for future drawing.
use super::Gdi;
#[derive(Clone, Copy)]
#[repr(u32)]
pub(super) enum BackgroundMode {
    Transparent = 1,
    Opaque = 2,
}
#[derive(Clone, Copy)]
pub(super) struct ColorRef(u32);
/// COLORREF is retained verbatim, including palette-index/RGB tags, until realization.
impl ColorRef {
    pub const BLACK: Self = Self(0);
    pub const WHITE: Self = Self(0x00ff_ffff);
}
pub(crate) enum ColorAttribute {
    Background,
    Text,
}
impl Gdi {
    pub fn background_mode(
        &mut self,
        handle: u32,
        new: Option<u32>,
    ) -> std::result::Result<u32, u32> {
        let context = self.contexts.get_mut(&handle).ok_or(6u32)?;
        let previous = context.background_mode;
        if let Some(mode) = new {
            context.background_mode = match mode {
                1 => BackgroundMode::Transparent,
                2 => BackgroundMode::Opaque,
                _ => return Err(87),
            };
        }
        Ok(previous as u32)
    }
    pub fn color(
        &mut self,
        handle: u32,
        attribute: ColorAttribute,
        new: Option<u32>,
    ) -> std::result::Result<u32, u32> {
        let context = self.contexts.get_mut(&handle).ok_or(6u32)?;
        let field = match attribute {
            ColorAttribute::Background => &mut context.background_color,
            ColorAttribute::Text => &mut context.text_color,
        };
        let previous = *field;
        if let Some(color) = new {
            *field = ColorRef(color);
        }
        Ok(previous.0)
    }
}
