//! Original GDI handle/selection contracts; no unimplemented font or bitmap drawing is advertised.
use super::*;
use crate::{Error, GuestMemory, Process, Result};
impl Gdi {
    pub(super) fn new_object(&mut self, object: objects::Object) -> Result<u32> {
        if self.objects.len() >= 4096 {
            return Err(Error::OutOfMemory);
        }
        self.object_serial = self
            .object_serial
            .checked_add(1)
            .filter(|n| *n < 0x1000000)
            .ok_or(Error::OutOfMemory)?;
        let handle = 0x0f000000 | self.object_serial;
        self.objects.insert(handle, object);
        Ok(handle)
    }
    pub fn compatibility<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
        thread: u32,
    ) -> Result<u32> {
        match name {
            "CreateDIBSection" => self.create_dib(p, a),
            "CreateCompatibleDC" => {
                let handle = self
                    .acquire(0, thread)
                    .map_err(|e| Error::Host(format!("DC allocation error {e}")))?;
                self.contexts.get_mut(&handle).unwrap().compatible = true;
                Ok(handle)
            }
            "DeleteDC" => {
                if self.contexts.get(&a[0]).is_some_and(|dc| dc.compatible) {
                    self.contexts.remove(&a[0]);
                    Ok(1)
                } else {
                    Ok(0)
                }
            }
            "SelectObject" => {
                let fallback = self.stock_object(4).map_err(|_| Error::InvalidAllocation)?;
                let Some(dc) = self.contexts.get_mut(&a[0]) else {
                    return Ok(0);
                };
                let previous = if dc.selected == 0 {
                    fallback
                } else {
                    dc.selected
                };
                if a[1] != 0 {
                    dc.selected = a[1];
                }
                Ok(previous)
            }
            "CreateFontA" => self.new_object(objects::Object::Font),
            "CreateSolidBrush" => self.new_object(objects::Object::Brush {
                style: 0,
                color: a[0],
            }),
            "UnrealizeObject" => Ok(1),
            "GetSystemPaletteEntries" => Ok(0),
            "DPtoLP" => Ok(u32::from(a[2] == 0 || a[1] != 0)),
            _ => Err(Error::Host(format!(
                "unknown GDI compatibility call {name}"
            ))),
        }
    }
}
