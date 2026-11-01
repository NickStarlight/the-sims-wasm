//! Window-long storage is owned by the window, including unaligned extra-byte offsets.
use super::*;

impl User {
    pub(super) fn window_long(&mut self, hwnd: u32, index: i32, replacement: Option<u32>) -> u32 {
        let Some(window) = self.windows.get_mut(&hwnd) else {
            return self.fail(1400);
        };
        if index >= 0 {
            let offset = index as usize;
            let Some(bytes) = window.extra.get_mut(offset..offset + 4) else {
                return self.fail(1413);
            };
            let previous = u32::from_le_bytes(bytes.try_into().unwrap());
            if let Some(value) = replacement {
                bytes.copy_from_slice(&value.to_le_bytes());
            }
            return previous;
        }
        let field = match index {
            -4 => &mut window.procedure,
            -6 => &mut window.instance,
            -16 => &mut window.style,
            -20 => &mut window.ex_style,
            -21 => &mut window.user_data,
            -8 if replacement.is_none() => {
                return if window.style & 0x4000_0000 != 0 {
                    window.parent
                } else {
                    window.owner
                }
            }
            -8 => return self.fail(120), // Reparenting/owner changes need their own lifecycle.
            _ => return self.fail(1413),
        };
        let previous = *field;
        if let Some(value) = replacement {
            if index == -4 && value == 0 {
                return self.fail(87);
            }
            // Geometry-changing styles are outside the borderless window model.
            if (index == -16 && (previous ^ value) & 0x40fc_0000 != 0)
                || (index == -20 && (previous ^ value) & !0x0804_00a8 != 0)
            {
                return self.fail(120);
            }
            *field = value;
            if index == -16 {
                window.visible = value & 0x1000_0000 != 0;
                if !window.visible && self.focus == Some(hwnd) {
                    self.focus = None;
                }
            }
        }
        previous
    }
}

#[cfg(test)]
#[path = "../../tests/unit/user/properties.rs"]
mod tests;
