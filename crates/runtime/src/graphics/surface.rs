//! Surface locking, attachment references and presentation.
use super::*;
impl Graphics {
    pub(super) fn surface_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let Some(s) = self.surfaces.get(&a[0]) else {
            return Ok(INVALID);
        };
        match name {
            "Surface_AddAttachedSurface" | "Surface_DeleteAttachedSurface" => {
                self.attached_call(p, name, a)
            }
            "Surface_Initialize" => Ok(0x88760005),
            "Surface_GetPrivateData" => Ok(0x80004005),
            "Surface_GetUniquenessValue" => {
                if a[1] != 0 {
                    write_u32(p, a[1], 1)?;
                }
                Ok(0)
            }
            "Surface_Lock" => self.lock(p, a),
            "Surface_Unlock" => {
                let s = self.surfaces.get_mut(&a[0]).unwrap();
                let Some(readonly) = s.lock else {
                    return Ok(INVALID);
                };
                if !readonly {
                    // A shared completed image must remain immutable. When a
                    // retired buffer is available, import directly into it rather
                    // than cloning old pixels that this full read would overwrite.
                    if Rc::strong_count(&s.pixels) > 1
                        && self
                            .recycled_pixels
                            .as_ref()
                            .is_some_and(|pixels| pixels.len() == s.pixels.len())
                    {
                        let mut pixels = self.recycled_pixels.take().unwrap();
                        s.guest_pixels_current = false;
                        if let Err(error) = p.read(s.guest_pixels, &mut pixels) {
                            self.recycled_pixels = Some(pixels);
                            return Err(error);
                        }
                        s.replace_pixels(Rc::new(pixels));
                    } else {
                        p.read(s.guest_pixels, s.pixels_mut().as_mut_slice())?;
                    }
                    s.guest_pixels_current = true;
                }
                s.lock = None;
                self.present(a[0])?;
                Ok(0)
            }
            "Surface_GetSurfaceDesc" => {
                let size = read_u32(p, a[1])?;
                if !matches!(size, 108 | 124) {
                    return Ok(INVALID);
                }
                p.write(a[1], &format::descriptor(s, size, 0, s.width, s.height))?;
                Ok(0)
            }
            "Surface_GetPixelFormat" => {
                p.write(a[1], &s.pixel_format.unwrap_or_else(|| s.format.record()))?;
                Ok(0)
            }
            "Surface_GetCaps" => {
                write_u32(
                    p,
                    a[1],
                    if s.primary {
                        0x220
                    } else if s.front != 0 {
                        0x14
                    } else {
                        s.caps
                    },
                )?;
                Ok(0)
            }
            "Surface_IsLost"
            | "Surface_Restore"
            | "Surface_GetBltStatus"
            | "Surface_GetFlipStatus" => Ok(0),
            "Surface_GetAttachedSurface" => {
                let caps = read_u32(p, a[1])?;
                let id = if caps & 0x20000 != 0 {
                    s.depth
                } else if caps & 4 != 0 {
                    s.back
                } else if caps & 0x20 != 0 {
                    s.front
                } else {
                    0
                };
                if id == 0 {
                    write_u32(p, a[2], 0)?;
                    return Ok(0x887600ff);
                }
                self.add_ref(id);
                write_u32(p, a[2], id)?;
                Ok(0)
            }
            "Surface_GetPalette" | "Surface_GetClipper" => {
                let id = if name == "Surface_GetPalette" {
                    s.palette
                } else {
                    s.clipper
                };
                write_u32(p, a[1], id)?;
                if id == 0 {
                    return Ok(0x887600ff);
                }
                self.add_ref(id);
                Ok(0)
            }
            "Surface_SetPalette" | "Surface_SetClipper" => self.attach(p, name, a),
            "Surface_SetColorKey" => self.color_key(p, a, true),
            "Surface_GetColorKey" => self.color_key(p, a, false),
            "Surface_BltBatch" => self.blit_batch(p, a),
            "Surface_Flip" => self.flip(a),
            "Surface_Blt" | "Surface_BltFast" => {
                let result = self.blit(p, name, a)?;
                if result == 0 {
                    self.present(a[0])?;
                }
                Ok(result)
            }
            _ => Ok(NOT_IMPLEMENTED),
        }
    }
    fn lock<M: GuestMemory>(&mut self, p: &mut Process<M>, a: &[u32]) -> Result<u32> {
        let s = self.surfaces.get_mut(&a[0]).unwrap();
        if s.lock.is_some() {
            return Ok(BUSY);
        }
        let size = read_u32(p, a[2])?;
        if !matches!(size, 108 | 124) {
            return Ok(INVALID);
        }
        let rect = raster::rectangle(p, a[1], s.width, s.height)?;
        if !rect.within(s.width, s.height) {
            return Ok(INVALID);
        }
        if s.guest_pixels == 0 {
            s.guest_pixels = p.heap_alloc(s.pixels.len() as u32)?;
        }
        // Guest access is valid only while locked. After Unlock the two copies
        // agree until a runtime pixel mutation invalidates this synchronization.
        if !s.guest_pixels_current {
            p.write(s.guest_pixels, &s.pixels)?;
            s.guest_pixels_current = true;
        }
        // Ported Lock validates a subrectangle but returns the full descriptor and base.
        p.write(
            a[2],
            &format::descriptor(s, size, s.guest_pixels, s.width, s.height),
        )?;
        s.lock = Some(false);
        Ok(0)
    }
    fn attach<M: GuestMemory>(&mut self, p: &mut Process<M>, name: &str, a: &[u32]) -> Result<u32> {
        let palette = name == "Surface_SetPalette";
        if a[1] != 0
            && if palette {
                !self.palettes.contains_key(&a[1])
            } else {
                !self.clippers.contains_key(&a[1])
            }
        {
            return Ok(INVALID);
        }
        if palette && self.surfaces[&a[0]].format.bits != 8 {
            return Ok(INVALID);
        }
        let s = self.surfaces.get_mut(&a[0]).unwrap();
        let field = if palette {
            &mut s.palette
        } else {
            &mut s.clipper
        };
        let old = *field;
        *field = a[1];
        self.add_ref(a[1]);
        if old != 0 {
            self.release(p, old)?;
        }
        self.present(a[0])?;
        Ok(0)
    }
    fn flip(&mut self, a: &[u32]) -> Result<u32> {
        let front = if self.surfaces[&a[0]].front != 0 {
            self.surfaces[&a[0]].front
        } else {
            a[0]
        };
        let back = self.surfaces[&front].back;
        if back == 0 || a[1] != 0 && a[1] != back {
            return Ok(0x88760246);
        }
        let pixels = self.surfaces[&back].pixels.clone();
        let old = self
            .surfaces
            .get_mut(&front)
            .unwrap()
            .replace_pixels(pixels);
        self.surfaces.get_mut(&back).unwrap().replace_pixels(old);
        self.present(front)?;
        Ok(0)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/graphics/surface.rs"]
mod tests;
