//! Legacy viewport normalization and stored v1/v2 records, without packed private pointers.
use super::*;
#[derive(Clone)]
pub(super) struct Viewport {
    pub rect: [u32; 4],
    pub min_z: f64,
    pub max_z: f64,
    pub scale: [f64; 3],
    pub offset: [f64; 3],
    pub background: u32,
    pub device: u32,
    pub lights: Vec<u32>,
    records: [[u8; 44]; 2],
}
impl Default for Viewport {
    fn default() -> Self {
        Self {
            rect: [0; 4],
            min_z: 0.,
            max_z: 1.,
            scale: [1.; 3],
            offset: [0.; 3],
            background: 0,
            device: 0,
            lights: Vec::new(),
            records: [[0; 44]; 2],
        }
    }
}
impl Viewport {
    pub fn transfer<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        address: u32,
        second: bool,
        set: bool,
    ) -> Result<u32> {
        if address == 0 || read_u32(p, address)? != 44 {
            return Ok(INVALID);
        }
        let index = usize::from(second);
        if !set {
            p.write(address, &self.records[index])?;
            return Ok(0);
        }
        let mut bytes = [0; 44];
        p.read(address, &mut bytes)?;
        let width = resources::word(&bytes, 12);
        let height = resources::word(&bytes, 16);
        let min = resources::float(&bytes, 36);
        let max = resources::float(&bytes, 40);
        let params: [f64; 4] = std::array::from_fn(|i| resources::float(&bytes, 20 + i * 4));
        if width == 0
            || height == 0
            || !min.is_finite()
            || !max.is_finite()
            || params.iter().any(|x| !x.is_finite())
            || second && (min < 0. || max > 1. || min >= max || params[2] == 0. || params[3] == 0.)
        {
            return Ok(INVALID);
        }
        self.rect = std::array::from_fn(|i| resources::word(&bytes, 4 + i * 4));
        self.min_z = 0.;
        self.max_z = 1.;
        (self.scale, self.offset) = if second {
            (
                [2. / params[2], 2. / params[3], 1. / (max - min)],
                [
                    -2. * params[0] / params[2] - 1.,
                    -2. * params[1] / params[3] + 1.,
                    -min / (max - min),
                ],
            )
        } else {
            (
                [
                    2. * params[0] / f64::from(width),
                    2. * params[1] / f64::from(height),
                    1.,
                ],
                [0.; 3],
            )
        };
        if !second {
            bytes[36..40].copy_from_slice(&0f32.to_le_bytes());
            bytes[40..44].copy_from_slice(&1f32.to_le_bytes());
        }
        self.records[index] = bytes;
        Ok(0)
    }
    pub fn projection(&self) -> [f64; 10] {
        [
            f64::from(self.rect[0]),
            f64::from(self.rect[1]),
            f64::from(self.rect[2]),
            f64::from(self.rect[3]),
            self.min_z,
            self.max_z,
            -1.,
            1.,
            2.,
            2.,
        ]
    }
}
impl Graphics {
    pub(super) fn d3d_viewport_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        if !self.d3d.viewports.contains_key(&a[0]) {
            return Ok(INVALID);
        }
        match name {
            "D3DViewport_GetViewport"
            | "D3DViewport_SetViewport"
            | "D3DViewport_GetViewport2"
            | "D3DViewport_SetViewport2" => self.d3d.viewports.get_mut(&a[0]).unwrap().transfer(
                p,
                a[1],
                name.ends_with('2'),
                name.contains("_Set"),
            ),
            "D3DViewport_SetBackground" => {
                if a[1] != 0 && !self.d3d.materials.values().any(|m| m.handle == a[1]) {
                    return Ok(INVALID);
                }
                self.d3d.viewports.get_mut(&a[0]).unwrap().background = a[1];
                Ok(0)
            }
            "D3DViewport_GetBackground" => {
                if a[1] == 0 || a[2] == 0 {
                    return Ok(INVALID);
                }
                let value = self.d3d.viewports[&a[0]].background;
                write_u32(p, a[1], value)?;
                write_u32(p, a[2], u32::from(value != 0))?;
                Ok(0)
            }
            "D3DViewport_AddLight" => {
                let Some(light) = self.d3d.lights.get_mut(&a[1]) else {
                    return Ok(INVALID);
                };
                if light.viewport != 0 {
                    return Ok(INVALID);
                }
                light.viewport = a[0];
                self.d3d.viewports.get_mut(&a[0]).unwrap().lights.push(a[1]);
                self.add_ref(a[1]);
                Ok(0)
            }
            "D3DViewport_DeleteLight" => {
                let Some(light) = self.d3d.lights.get_mut(&a[1]) else {
                    return Ok(INVALID);
                };
                if light.viewport != a[0] {
                    return Ok(INVALID);
                }
                light.viewport = 0;
                self.d3d
                    .viewports
                    .get_mut(&a[0])
                    .unwrap()
                    .lights
                    .retain(|&x| x != a[1]);
                self.release(p, a[1])?;
                Ok(0)
            }
            "D3DViewport_NextLight" => {
                if a[2] == 0 {
                    return Ok(INVALID);
                }
                let lights = &self.d3d.viewports[&a[0]].lights;
                let next = match a[3] {
                    1 => lights
                        .iter()
                        .position(|&x| x == a[1])
                        .and_then(|i| lights.get(i + 1)),
                    2 => lights.first(),
                    4 => lights.last(),
                    _ => return Ok(INVALID),
                }
                .copied();
                write_u32(p, a[2], next.unwrap_or(0))?;
                if let Some(id) = next {
                    self.add_ref(id);
                    Ok(0)
                } else {
                    Ok(0x887600ff)
                }
            }
            "D3DViewport_Clear" | "D3DViewport_Clear2" => self.d3d_clear(p, name, a),
            "D3DViewport_GetBackgroundDepth" => {
                if a[1] != 0 {
                    write_u32(p, a[1], 0)?;
                }
                if a[2] != 0 {
                    write_u32(p, a[2], 0)?;
                }
                Ok(NOT_IMPLEMENTED)
            }
            _ => Ok(NOT_IMPLEMENTED),
        }
    }
}
