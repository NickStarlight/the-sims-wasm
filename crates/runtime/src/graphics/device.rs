//! Display configuration, surface creation and callback enumeration records.
use super::*;
impl Graphics {
    pub(super) fn device_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        if matches!(name, "DirectDrawCreate" | "DirectDrawCreateEx") {
            let outer = if name == "DirectDrawCreate" {
                args[2]
            } else {
                args[3]
            };
            if outer != 0 || args[1] == 0 {
                return Ok(INVALID);
            }
            write_u32(process, args[1], 0)?;
            let version = if name == "DirectDrawCreate" {
                1
            } else {
                let mut bytes = [0; 16];
                process.read(args[2], &mut bytes)?;
                let iid = std::array::from_fn(|i| {
                    u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap())
                });
                let Some((version, _)) = com::DEVICE_IDS.iter().find(|(_, known)| *known == iid)
                else {
                    return Ok(NO_INTERFACE);
                };
                *version
            };
            let object = self.device(process, modules, version)?;
            write_u32(process, args[1], object)?;
            return Ok(0);
        }
        let Some(Object {
            kind: Kind::Device(version),
            ..
        }) = self.objects.get(&args[0])
        else {
            return Ok(INVALID);
        };
        let version = *version;
        match name {
            "DDraw_DuplicateSurface" => self.duplicate_surface(process, modules, args),
            "DDraw_CreateSurface" => self.create_surface(process, modules, version, args),
            "DDraw_CreatePalette" => self.create_palette(process, modules, args),
            "DDraw_CreateClipper" => {
                if args[2] == 0 || args[3] != 0 {
                    return Ok(INVALID);
                }
                let id = self.object(process, modules, Kind::Clipper)?;
                self.clippers.insert(id, clipper::Clipper::default());
                write_u32(process, args[2], id)?;
                Ok(0)
            }
            "DDraw_SetCooperativeLevel" => {
                let flags = args[2];
                if flags & 0x18 == 0 || (flags & 0x10 != 0 && (flags & 1 == 0 || args[1] == 0)) {
                    return Ok(INVALID);
                }
                self.cooperative = flags;
                Ok(0)
            }
            "DDraw_SetDisplayMode" | "DDraw_SetDisplayModeEx" => {
                let Some(format) = format::Format::new(args[3]) else {
                    return Ok(INVALID);
                };
                if args[1] == 0
                    || args[2] == 0
                    || u64::from(args[1]) * u64::from(args[2]) * u64::from(format.bytes())
                        > MAX_BYTES as u64
                {
                    return Ok(INVALID);
                }
                if self
                    .surfaces
                    .values()
                    .any(|surface| surface.caps & 0x200 != 0)
                {
                    return Ok(BUSY);
                }
                self.display.set_mode((args[1], args[2], args[3]));
                Ok(0)
            }
            "DDraw_GetDisplayMode" => {
                let size = read_u32(process, args[1])?;
                if !matches!(size, 108 | 124) {
                    return Ok(INVALID);
                }
                let surface = self.mode_surface()?;
                process.write(
                    args[1],
                    &format::descriptor(&surface, size, 0, surface.width, surface.height),
                )?;
                Ok(0)
            }
            "DDraw_GetCaps" => {
                for address in [args[1], args[2]] {
                    if address != 0 {
                        let size = read_u32(process, address)?;
                        if !(8..=512).contains(&size) {
                            return Ok(INVALID);
                        }
                        let mut bytes = vec![0; size as usize];
                        bytes[..4].copy_from_slice(&size.to_le_bytes());
                        bytes[4..8].copy_from_slice(&0x04000040u32.to_le_bytes());
                        process.write(address, &bytes)?;
                    }
                }
                Ok(0)
            }
            "DDraw_GetAvailableVidMem" => {
                let used: usize = self.surfaces.values().map(|s| s.pixels.len()).sum();
                for address in [args[2], args[3]] {
                    if address != 0 {
                        write_u32(
                            process,
                            address,
                            if address == args[2] {
                                MAX_BYTES as u32
                            } else {
                                MAX_BYTES.saturating_sub(used) as u32
                            },
                        )?;
                    }
                }
                Ok(0)
            }
            "DDraw_GetFourCCCodes" | "DDraw_GetGDISurface" | "DDraw_GetScanLine" => {
                if args[1] != 0 {
                    write_u32(process, args[1], 0)?;
                }
                Ok(0)
            }
            "DDraw_GetSurfaceFromDC" => {
                if args[2] != 0 {
                    write_u32(process, args[2], 0)?;
                }
                Ok(0)
            }
            "DDraw_GetDeviceIdentifier" => {
                if args[1] != 0 {
                    let size = read_u32(process, args[1])?.min(4096);
                    process.write(args[1], &vec![0; size as usize])?;
                }
                Ok(0)
            }
            // Existing compatibility returns from old dd_device_call, not implemented GPU operations.
            "DDraw_Compact"
            | "DDraw_FlipToGDISurface"
            | "DDraw_Initialize"
            | "DDraw_RestoreAllSurfaces"
            | "DDraw_TestCooperativeLevel" => Ok(0),
            "DDraw_GetMonitorFrequency" => {
                write_u32(process, args[1], 60)?;
                Ok(0)
            }
            "DDraw_GetVerticalBlankStatus" => {
                write_u32(process, args[1], 0)?;
                Ok(0)
            }
            "DDraw_WaitForVerticalBlank" => Ok(0),
            "DDraw_RestoreDisplayMode" => {
                self.display.set_mode(crate::display::DEFAULT_MODE);
                Ok(0)
            }
            _ => Ok(NOT_IMPLEMENTED),
        }
    }
    fn mode_surface(&self) -> Result<Surface> {
        let (width, height, bits) = self.display.mode();
        let format = format::Format::new(bits).ok_or(Error::InvalidRange)?;
        Ok(Surface {
            width,
            height,
            pitch: width * format.bytes(),
            format,
            caps: 0,
            primary: false,
            pixels: Rc::new(Vec::new()),
            lock: None,
            guest_pixels: 0,
            guest_pixels_current: false,
            dc: 0,
            palette: 0,
            clipper: 0,
            back: 0,
            front: 0,
            depth: 0,
            pixel_format: None,
            source_overlay_key: None,
            destination_overlay_key: None,
            source_key: None,
            destination_key: None,
        })
    }
    fn create_surface<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        version: u32,
        args: &[u32],
    ) -> Result<u32> {
        if args[1] == 0 || args[2] == 0 || args[3] != 0 {
            return Ok(INVALID);
        }
        let size = read_u32(process, args[1])?;
        if size != if version >= 4 { 124 } else { 108 } {
            return Ok(INVALID);
        }
        let mut desc = vec![0; size as usize];
        process.read(args[1], &mut desc)?;
        let word = |offset: usize| u32::from_le_bytes(desc[offset..offset + 4].try_into().unwrap());
        let flags = word(4);
        let caps = word(104);
        if caps & 0x24 != 0 {
            return Ok(0x88760064);
        }
        let width = if flags & 4 != 0 {
            word(12)
        } else {
            self.display.mode().0
        };
        let height = if flags & 2 != 0 {
            word(8)
        } else {
            self.display.mode().1
        };
        let legacy_depth = caps & 0x20000 != 0 && flags & 0x40 != 0 && flags & 0x1000 == 0;
        let bits = if legacy_depth {
            word(24)
        } else if flags & 0x1000 != 0 {
            word(84)
        } else {
            self.display.mode().2
        };
        let mut pixel_format = if flags & 0x1000 != 0 {
            Some(desc[72..104].try_into().unwrap())
        } else {
            None
        };
        if caps & 0x20000 != 0 {
            if bits != 16 || (!legacy_depth && (word(76) & 0x400 == 0 || word(92) != 0xffff)) {
                write_u32(process, args[2], 0)?;
                return Ok(NOT_IMPLEMENTED);
            }
            if legacy_depth {
                let words = [32u32, 0x400, 0, 16, 0, 0xffff, 0, 0];
                let mut bytes = [0; 32];
                for (i, word) in words.iter().enumerate() {
                    bytes[i * 4..i * 4 + 4].copy_from_slice(&word.to_le_bytes());
                }
                pixel_format = Some(bytes);
            }
        }
        let Some(mut format) = format::Format::new(bits) else {
            return Ok(INVALID);
        };
        if flags & 0x1000 != 0 && bits != 8 && word(88) != 0 {
            format.red = word(88);
            format.green = word(92);
            format.blue = word(96);
        }
        let Some(pitch) = width.checked_mul(format.bytes()) else {
            return Ok(INVALID);
        };
        let length = u64::from(pitch) * u64::from(height);
        if width == 0 || height == 0 || length > MAX_BYTES as u64 {
            return Ok(INVALID);
        }
        let buffers = if caps & 0x210 == 0x210 { 2 } else { 1 };
        let used: usize = self.surfaces.values().map(|s| s.pixels.len()).sum();
        if length as usize * buffers > MAX_BYTES.saturating_sub(used) {
            return Err(Error::OutOfMemory);
        }
        write_u32(process, args[2], 0)?;
        let pixels = Rc::new(zero_bytes(length as usize)?);
        let id = self.object(process, modules, Kind::Surface)?;
        let mut surface = Surface {
            width,
            height,
            pitch,
            format,
            caps,
            primary: caps & 0x200 != 0,
            pixels,
            lock: None,
            guest_pixels: 0,
            guest_pixels_current: false,
            dc: 0,
            palette: 0,
            clipper: 0,
            back: 0,
            front: 0,
            depth: 0,
            pixel_format,
            source_overlay_key: None,
            destination_overlay_key: None,
            source_key: None,
            destination_key: None,
        };
        if caps & 0x210 == 0x210 {
            let back_id = self.object(process, modules, Kind::Surface)?;
            let mut back = surface.clone();
            back.pixels = Rc::new(zero_bytes(length as usize)?);
            back.primary = false;
            back.front = id;
            surface.back = back_id;
            self.surfaces.insert(back_id, back);
        }
        self.surfaces.insert(id, surface);
        write_u32(process, args[2], id)?;
        Ok(0)
    }
    pub fn enumerate<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<Vec<(u32, Vec<u32>)>> {
        let mut calls = Vec::new();
        if name == "DirectDrawEnumerateA" {
            if args[0] == 0 {
                return Err(Error::Host(
                    "DirectDraw enumeration requires a callback".into(),
                ));
            }
            let desc = process.heap_alloc(32)?;
            process.write(desc, b"Software DirectDraw\0")?;
            let driver = process.heap_alloc(16)?;
            process.write(driver, b"display\0")?;
            self.enumeration_buffers.extend([desc, driver]);
            calls.push((args[0], vec![0, desc, driver, args[1]]));
        } else {
            let Some(Object {
                kind: Kind::Device(version),
                ..
            }) = self.objects.get(&args[0])
            else {
                return Err(Error::Host("invalid DirectDraw enumeration object".into()));
            };
            let version = *version;
            if args[4] == 0 {
                return Err(Error::Host(
                    "display mode enumeration requires a callback".into(),
                ));
            }
            for (width, height, bits) in crate::display::MODES {
                if args[2] != 0 {
                    let flags = read_u32(process, args[2] + 4)?;
                    if flags & 4 != 0 && read_u32(process, args[2] + 12)? != width
                        || flags & 2 != 0 && read_u32(process, args[2] + 8)? != height
                        || flags & 0x1000 != 0 && read_u32(process, args[2] + 84)? != bits
                    {
                        continue;
                    }
                }
                let mut surface = self.mode_surface()?;
                surface.width = width;
                surface.height = height;
                surface.format = format::Format::new(bits).unwrap();
                surface.pitch = (width * surface.format.bytes() + 3) & !3;
                let size = if version >= 4 { 124 } else { 108 };
                let pointer = process.heap_alloc(size)?;
                process.write(
                    pointer,
                    &format::descriptor(&surface, size, 0, width, height),
                )?;
                self.enumeration_buffers.push(pointer);
                calls.push((args[4], vec![pointer, args[3]]));
            }
        }
        Ok(calls)
    }
}
