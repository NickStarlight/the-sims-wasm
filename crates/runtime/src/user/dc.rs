//! Window validation and ownership for USER32's display DC leases.
use super::*;
impl User {
    pub(crate) fn acquire_surface_dc(
        &mut self,
        surface: u32,
        thread: u32,
    ) -> std::result::Result<u32, u32> {
        self.gdi.acquire_surface(surface, thread)
    }
    pub(crate) fn release_surface_dc(&mut self, surface: u32, dc: u32, thread: u32) -> bool {
        self.gdi.release_surface(surface, dc, thread)
    }

    pub(crate) fn dc_call(&mut self, name: &str, args: &[u32], thread: u32) -> Result<u32> {
        match name {
            "GetDC" => {
                if args[0] != 0 && !self.windows.contains_key(&args[0]) {
                    return Ok(self.fail(1400));
                }
                match self.gdi.acquire(args[0], thread) {
                    Ok(handle) => Ok(handle),
                    Err(error) => Ok(self.fail(error)),
                }
            }
            "SelectPalette" => match self.gdi.select_palette(args[0], args[1], args[2] != 0) {
                Ok(previous) => Ok(previous),
                Err(error) => Ok(self.fail(error)),
            },
            "RealizePalette" => match self.gdi.realize_palette(args[0], self.display.mode().2) {
                Ok(count) => Ok(count),
                Err(error) => {
                    self.fail(error);
                    Ok(u32::MAX)
                }
            },
            "SetBkMode" | "GetBkMode" => {
                match self.gdi.background_mode(args[0], args.get(1).copied()) {
                    Ok(mode) => Ok(mode),
                    Err(error) => Ok(self.fail(error)),
                }
            }
            "SetBkColor" | "GetBkColor" | "SetTextColor" | "GetTextColor" => {
                let attribute = if name.ends_with("BkColor") {
                    crate::gdi::ColorAttribute::Background
                } else {
                    crate::gdi::ColorAttribute::Text
                };
                match self.gdi.color(args[0], attribute, args.get(1).copied()) {
                    Ok(color) => Ok(color),
                    Err(error) => {
                        self.fail(error);
                        Ok(u32::MAX)
                    }
                }
            }
            "SetMapMode" => match self.gdi.set_map_mode(args[0], args[1]) {
                Ok(previous) => Ok(previous),
                Err(error) => Ok(self.fail(error)),
            },
            "GetMapMode" => match self.gdi.map_mode(args[0]) {
                Ok(mode) => Ok(mode),
                Err(error) => Ok(self.fail(error)),
            },
            "ReleaseDC" => Ok(u32::from(self.gdi.release(args[0], args[1], thread))),
            "GetDeviceCaps" => match self.gdi.caps(args[0], args[1], self.display.mode()) {
                Ok(value) => Ok(value),
                Err(error) => Ok(self.fail(error)),
            },
            _ => Err(Error::Host(format!("unsupported DC service {name}"))),
        }
    }
}

impl User {
    pub(crate) fn object_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        match name {
            "CreatePalette" => {
                let mut header = [0; 4];
                process.read(args[0], &mut header)?;
                let version = u16::from_le_bytes([header[0], header[1]]);
                let count = u16::from_le_bytes([header[2], header[3]]) as usize;
                if version != 0x300 || !(1..=256).contains(&count) {
                    return Ok(self.fail(87));
                }
                let address = args[0].checked_add(4).ok_or(Error::InvalidRange)?;
                let mut bytes = vec![0; count * 4];
                process.read(address, &mut bytes)?;
                match self.gdi.create_palette(bytes.as_chunks::<4>().0.to_vec()) {
                    Ok(handle) => Ok(handle),
                    Err(error) => Ok(self.fail(error)),
                }
            }
            "GetPaletteEntries" => {
                let entries = match self.gdi.palette_entries(args[0]) {
                    Ok(entries) => entries,
                    Err(error) => return Ok(self.fail(error)),
                };
                if args[3] == 0 {
                    return Ok(entries.len() as u32);
                }
                let start = (args[1] as usize).min(entries.len());
                let count = (args[2] as usize).min(entries.len() - start);
                let bytes: Vec<_> = entries[start..start + count]
                    .iter()
                    .flatten()
                    .copied()
                    .collect();
                process.write(args[3], &bytes)?;
                Ok(count as u32)
            }
            "GetStockObject" => match self.gdi.stock_object(args[0]) {
                Ok(handle) => Ok(handle),
                Err(error) => Ok(self.fail(error)),
            },
            "DeleteObject" => {
                if self.gdi.delete_owned_object(process, args[0])? {
                    Ok(1)
                } else {
                    Ok(self.fail(6))
                }
            }
            "GetObjectA" => {
                if let Some(record) = self.gdi.bitmap_record(args[0]) {
                    if args[2] == 0 {
                        return Ok(24);
                    }
                    let size = if args[1] >= 44 {
                        44
                    } else if args[1] >= 24 {
                        24
                    } else {
                        return Ok(0);
                    };
                    process.write(args[2], &record[..size])?;
                    return Ok(size as u32);
                }

                let record = match self.gdi.object_record(args[0]) {
                    Ok(record) => record,
                    Err(error) => return Ok(self.fail(error)),
                };
                if args[2] == 0 {
                    return Ok(record.len() as u32);
                }
                if args[1] as i32 <= 0 || args[2] & 3 != 0 {
                    return Ok(self.fail(87));
                }
                let count = (args[1] as usize).min(record.len());
                process.write(args[2], &record[..count])?;
                Ok(count as u32)
            }
            _ => Err(Error::Host(format!(
                "unsupported GDI object service {name}"
            ))),
        }
    }
}

impl User {
    pub(crate) fn gdi_compatibility<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
        thread: u32,
    ) -> Result<u32> {
        self.gdi.compatibility(p, name, a, thread)
    }
}
