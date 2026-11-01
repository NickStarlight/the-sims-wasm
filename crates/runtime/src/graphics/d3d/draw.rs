//! Primitive assembly and viewport clearing ported from the working D3D renderer.
use super::*;
impl Graphics {
    pub(super) fn d3d_draw<M: GuestMemory>(
        &mut self,
        p: &Process<M>,
        a: &[u32],
        indexed: bool,
        legacy: bool,
    ) -> Result<u32> {
        let id = self.d3d.device_id(a[0]).ok_or(Error::InvalidAllocation)?;
        let device = &self.d3d.devices[&id];
        let Some(view) = self.d3d.viewports.get(&device.viewport).cloned() else {
            return Ok(INVALID);
        };
        if !device.in_scene || a[3] == 0 || a[4] == 0 {
            return Ok(INVALID);
        }
        let fvf = if legacy {
            match a[2] {
                1 => 0x112,
                3 => 0x1c4,
                _ => return Ok(NOT_IMPLEMENTED),
            }
        } else {
            a[2]
        };
        let transformed = fvf & 14 == 4;
        if !(4..=6).contains(&a[1])
            || (!transformed && fvf & 14 != 2)
            || fvf & !0x1d6 != 0
            || (transformed && fvf & 16 != 0)
        {
            return Ok(NOT_IMPLEMENTED);
        }
        let count = if indexed { a[6] } else { a[4] };
        if (indexed && count != 0 && a[5] == 0) || (a[1] == 4 && !count.is_multiple_of(3)) {
            return Ok(INVALID);
        }
        let stride = vertices::stride(fvf);
        let size = (a[4] as usize)
            .checked_mul(stride)
            .filter(|size| *size <= MAX_BYTES)
            .ok_or(Error::InvalidRange)?;
        let mut bytes = vec![0; size];
        p.read(a[3], &mut bytes)?;
        if count as usize > MAX_BYTES / 2 {
            return Ok(INVALID);
        }
        let indices: Vec<usize> = if indexed {
            let mut bytes = vec![0; count as usize * 2];
            p.read(a[5], &mut bytes)?;
            bytes
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| u16::from_le_bytes(*b) as usize)
                .collect()
        } else {
            (0..count as usize).collect()
        };
        if indices.iter().any(|i| *i >= a[4] as usize) {
            return Ok(INVALID);
        }
        let material = self
            .d3d
            .materials
            .values()
            .find(|m| Some(m.handle) == device.light_states.get(1))
            .map(vertices::material);
        let lights: Vec<_> = view
            .lights
            .iter()
            .filter_map(|id| self.d3d.lights.get(id))
            .map(vertices::light)
            .collect();
        if !transformed && lights.iter().any(|l| l.enabled && l.kind > 4) {
            return Ok(NOT_IMPLEMENTED);
        }
        let points: Vec<_> = bytes
            .chunks_exact(stride)
            .map(|b| vertices::decode(b, fvf, device, &view, material.as_ref(), &lights))
            .collect();
        let texture = self
            .d3d
            .textures
            .get(&device.texture)
            .filter(|_| device.texture_states.get(1) != Some(1))
            .and_then(|id| self.surfaces.get(id))
            .cloned();
        let palette = texture
            .as_ref()
            .and_then(|t| self.palettes.get(&t.palette))
            .map(|p| p.entries);
        if texture.as_ref().is_some_and(|t| {
            !texture::supported(t)
                || (t.format.bits == 8 && palette.is_none())
                || device.texture_states.get(16) != device.texture_states.get(17)
        }) {
            return Ok(NOT_IMPLEMENTED);
        }
        let get = |k| device.render.get(k).unwrap_or(0);
        let stage = |k| device.texture_states.get(k).unwrap_or(0);
        let options = raster::Options {
            clip: [
                view.rect[0] as f64,
                view.rect[1] as f64,
                view.rect[0] as f64 + view.rect[2] as f64,
                view.rect[1] as f64 + view.rect[3] as f64,
            ],
            cull: get(22),
            z_enable: get(7),
            z_write: get(14),
            z_func: get(23),
            alpha_blend: get(27),
            src_blend: get(19),
            dst_blend: get(20),
            alpha_test: get(15),
            alpha_ref: get(24),
            alpha_func: get(25),
            stage: [
                stage(1),
                stage(2),
                stage(3),
                stage(4),
                stage(5),
                stage(6),
                device.texture_factor,
            ],
            texture: texture.as_ref().map(|t| texture::TextureSampler {
                pixels: &t.pixels,
                width: t.width,
                height: t.height,
                pitch: t.pitch,
                bpp: t.format.bits,
                masks: texture::masks(t),
                palette: palette.as_ref().map(|p| p.as_slice()),
                color_key: if device.color_key { t.source_key } else { None },
                address_u: stage(13),
                address_v: stage(14),
                linear: stage(16) == 2,
                border: stage(15),
            }),
        };
        let mut extents = device.clip_extents;
        let update = a[if indexed { 7 } else { 5 }] & 8 == 0;
        let mut triangles = 0u32;
        self.with_d3d_target(id, |target| {
            let mut emit = |i: usize, j: usize, k: usize| {
                let triangle = [points[indices[i]], points[indices[j]], points[indices[k]]];
                let polygon: Vec<[f64; 7]> = if transformed {
                    triangle
                        .iter()
                        .map(|p| std::array::from_fn(|i| p[i]))
                        .collect()
                } else {
                    math::clip(&triangle)
                        .iter()
                        .map(|v| math::project(*v, &view.projection()))
                        .collect()
                };
                if update {
                    if let Some(bounds) = extents.as_mut() {
                        for p in &polygon {
                            bounds[0] = bounds[0].min(p[0].max(options.clip[0]));
                            bounds[1] = bounds[1].max(p[0].min(options.clip[2]));
                            bounds[2] = bounds[2].min(p[1].max(options.clip[1]));
                            bounds[3] = bounds[3].max(p[1].min(options.clip[3]));
                            bounds[4] = bounds[4].min(p[2]);
                            bounds[5] = bounds[5].max(p[2]);
                        }
                    }
                }
                for n in 1..polygon.len().saturating_sub(1) {
                    raster::triangle(target, &options, [polygon[0], polygon[n], polygon[n + 1]]);
                }
                triangles = triangles.wrapping_add(1);
            };
            match a[1] {
                4 => {
                    for i in (0..count as usize).step_by(3) {
                        emit(i, i + 1, i + 2);
                    }
                }
                5 => {
                    for i in 0..(count as usize).saturating_sub(2) {
                        emit(i + (i & 1), i + 1 - (i & 1), i + 2);
                    }
                }
                6 => {
                    for i in 1..(count as usize).saturating_sub(1) {
                        emit(0, i, i + 1);
                    }
                }
                _ => {}
            }
        })?;
        let device = self.d3d.devices.get_mut(&id).unwrap();
        device.triangles = device.triangles.wrapping_add(triangles);
        device.clip_extents = extents;
        Ok(0)
    }

    pub(super) fn with_d3d_target(
        &mut self,
        id: u32,
        draw: impl FnOnce(&mut raster::Target),
    ) -> Result<()> {
        let device = self
            .d3d
            .devices
            .get_mut(&id)
            .ok_or(Error::InvalidAllocation)?;
        let surface = self
            .surfaces
            .get(&device.target)
            .ok_or(Error::InvalidAllocation)?;
        let count = surface.width as usize * surface.height as usize;
        if device.depth.len() != count {
            device.depth = vec![1.; count];
        }
        let depth_id = surface.depth;
        let mut depth = self.surfaces.remove(&depth_id);
        let surface = self.surfaces.get_mut(&device.target).unwrap();
        let mut target = raster::Target {
            width: surface.width,
            height: surface.height,
            pitch: surface.pitch,
            bpp: surface.format.bits,
            pixels: surface.pixels_mut().as_mut_slice(),
            depth: &mut device.depth,
            native_depth: depth.as_mut().map(|d| {
                let pitch = d.pitch;
                (d.pixels_mut().as_mut_slice(), pitch)
            }),
        };
        draw(&mut target);
        if let Some(depth) = depth {
            self.surfaces.insert(depth_id, depth);
        }
        Ok(())
    }
}
