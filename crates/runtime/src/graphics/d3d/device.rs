//! Device3 state and Device2 aliases ported from the working winapi/d3d-device*.ts adapters.
use super::*;
use std::collections::BTreeSet;

pub(super) struct Device {
    pub depth: Vec<f32>,
    pub parent: u32,
    pub target: u32,
    pub viewport: u32,
    pub attached: BTreeSet<u32>,
    pub in_scene: bool,
    pub triangles: u32,
    pub render: state::StateBank,
    pub texture_states: state::StateBank,
    pub light_states: state::StateBank,
    pub texture: u32,
    pub texture_factor: u32,
    pub color_key: bool,
    pub matrices: [[f64; 16]; 6],
    pub clip_extents: Option<[f64; 6]>,
    pub clip_status: u32,
}
impl Device {
    pub fn new(parent: u32, target: u32) -> Self {
        Self {
            depth: Vec::new(),
            parent,
            target,
            viewport: 0,
            attached: BTreeSet::new(),
            in_scene: false,
            triangles: 0,
            render: state::StateBank::new(0),
            texture_states: state::StateBank::new(1),
            light_states: state::StateBank::new(2),
            texture: 0,
            texture_factor: u32::MAX,
            color_key: false,
            matrices: [state::identity(); 6],
            clip_extents: None,
            clip_status: 0,
        }
    }
}
impl State {
    pub fn device_id(&self, id: u32) -> Option<u32> {
        if self.devices.contains_key(&id) {
            Some(id)
        } else {
            self.device_aliases.get(&id).copied()
        }
    }
}
impl Graphics {
    pub(super) fn d3d_device_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        _modules: &ModuleCatalog,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        let Some(id) = self.d3d.device_id(args[0]) else {
            return Ok(INVALID);
        };
        let legacy = name.starts_with("D3DDevice2_");
        let method = name.split_once('_').map_or(name, |(_, method)| method);
        if matches!(method, "DrawPrimitive" | "DrawIndexedPrimitive") {
            return self.d3d_draw(process, args, method == "DrawIndexedPrimitive", legacy);
        }
        if legacy
            && matches!(
                method,
                "SwapTextureHandles" | "EnumTextureFormats" | "GetDirect3D"
            )
        {
            return Ok(NOT_IMPLEMENTED);
        }
        match method {
            "AddViewport" | "DeleteViewport" => {
                self.device_viewport(id, args[1], method == "AddViewport")
            }
            "SetRenderTarget" => {
                if !self
                    .surfaces
                    .get(&args[1])
                    .is_some_and(|surface| matches!(surface.format.bits, 16 | 32))
                {
                    return Ok(INVALID);
                }
                self.d3d.devices.get_mut(&id).unwrap().target = args[1];
                Ok(0)
            }
            "GetClipStatus" => self.device_clip_status(process, id, args[1]),
            _ => {
                let known_texture = matches!(method, "SetTexture" | "SetRenderState")
                    && (args[2] == 0 || self.d3d.textures.contains_key(&args[2]));
                let known_material = method == "SetLightState"
                    && (args[2] == 0 || self.d3d.materials.values().any(|m| m.handle == args[2]));
                let device = self.d3d.devices.get_mut(&id).unwrap();
                device_call(process, device, method, args, known_texture, known_material)
            }
        }
    }
    fn device_viewport(&mut self, id: u32, viewport: u32, add: bool) -> Result<u32> {
        let Some(record) = self.d3d.viewports.get_mut(&viewport) else {
            return Ok(INVALID);
        };
        let device = self.d3d.devices.get_mut(&id).unwrap();
        if add {
            if device.attached.contains(&viewport) || (record.device != 0 && record.device != id) {
                return Ok(INVALID);
            }
            device.attached.insert(viewport);
            record.device = id;
        } else {
            if !device.attached.remove(&viewport) {
                return Ok(INVALID);
            }
            record.device = 0;
            if device.viewport == viewport {
                device.viewport = 0;
            }
        }
        Ok(0)
    }
    fn device_clip_status<M: GuestMemory>(
        &self,
        process: &mut Process<M>,
        id: u32,
        address: u32,
    ) -> Result<u32> {
        let device = &self.d3d.devices[&id];
        let Some(viewport) = self.d3d.viewports.get(&device.viewport) else {
            return Ok(INVALID);
        };
        if address == 0 {
            return Ok(INVALID);
        }
        let [x, y, width, height] = viewport.rect;
        let bounds = device.clip_extents.unwrap_or([
            f64::from(x),
            f64::from(x) + f64::from(width),
            f64::from(y),
            f64::from(y) + f64::from(height),
            0.0,
            1.0,
        ]);
        let mut bytes = [0; 32];
        bytes[..4].copy_from_slice(&3u32.to_le_bytes());
        bytes[4..8].copy_from_slice(&device.clip_status.to_le_bytes());
        for (slot, value) in bytes[8..].as_chunks_mut::<4>().0.iter_mut().zip(bounds) {
            slot.copy_from_slice(&(value as f32).to_le_bytes());
        }
        process.write(address, &bytes)?;
        Ok(0)
    }
}
fn output<M: GuestMemory>(
    process: &mut Process<M>,
    address: u32,
    value: Option<u32>,
) -> Result<u32> {
    let Some(value) = value else {
        return Ok(NOT_IMPLEMENTED);
    };
    if address == 0 {
        return Ok(INVALID);
    }
    process.write(address, &value.to_le_bytes())?;
    Ok(0)
}
fn device_call<M: GuestMemory>(
    process: &mut Process<M>,
    device: &mut Device,
    method: &str,
    a: &[u32],
    known_texture: bool,
    known_material: bool,
) -> Result<u32> {
    match method {
        "BeginScene" => {
            if device.in_scene {
                return Ok(INVALID);
            }
            device.in_scene = true;
            Ok(0)
        }
        "EndScene" => {
            if !device.in_scene {
                return Ok(INVALID);
            }
            device.in_scene = false;
            Ok(0)
        }
        "GetDirect3D" => output(process, a[1], Some(device.parent)),
        "GetRenderTarget" => output(process, a[1], Some(device.target)),
        "SetCurrentViewport" => {
            if !device.attached.contains(&a[1]) {
                return Ok(INVALID);
            }
            device.viewport = a[1];
            Ok(0)
        }
        "GetCurrentViewport" => output(process, a[1], Some(device.viewport)),
        "GetRenderState" => output(
            process,
            a[2],
            match a[1] {
                1 => Some(device.texture),
                60 => Some(device.texture_factor),
                41 => Some(u32::from(device.color_key)),
                key => device.render.get(key),
            },
        ),
        "SetRenderState" => set_render_state(device, a[1], a[2], known_texture),
        "GetLightState" => output(process, a[2], device.light_states.get(a[1])),
        "SetLightState" => Ok(device.light_states.set(a[1], a[2], known_material)),
        "SetTransform" | "GetTransform" => {
            matrix_transfer(process, device, a[1], a[2], method == "SetTransform")
        }
        "SetClipStatus" => set_clip_status(process, device, a[1]),
        "GetStats" => {
            if a[1] == 0 || read_u32(process, a[1])? != 24 {
                return Ok(INVALID);
            }
            let mut bytes = [0; 24];
            bytes[..4].copy_from_slice(&24u32.to_le_bytes());
            bytes[4..8].copy_from_slice(&device.triangles.to_le_bytes());
            process.write(a[1], &bytes)?;
            Ok(0)
        }
        "GetTexture" => {
            if a[1] != 0 {
                return Ok(NOT_IMPLEMENTED);
            }
            output(process, a[2], Some(device.texture))
        }
        "SetTexture" => {
            if a[1] != 0 {
                return Ok(NOT_IMPLEMENTED);
            }
            if !known_texture {
                return Ok(INVALID);
            }
            device.texture = a[2];
            Ok(0)
        }
        "GetTextureStageState" => output(
            process,
            a[3],
            if a[1] == 0 {
                device.texture_states.get(a[2])
            } else if a[2] == 1 {
                Some(1)
            } else {
                None
            },
        ),
        "SetTextureStageState" => {
            if a[1] != 0 {
                return Ok(if a[2] == 1 && a[3] == 1 {
                    0
                } else {
                    NOT_IMPLEMENTED
                });
            }
            Ok(device.texture_states.set(a[2], a[3], true))
        }
        "ValidateDevice" => output(process, a[1], Some(1)),
        _ => Ok(NOT_IMPLEMENTED),
    }
}
fn set_render_state(device: &mut Device, key: u32, value: u32, known_texture: bool) -> Result<u32> {
    match key {
        1 => {
            if !known_texture {
                return Ok(INVALID);
            }
            device.texture = value;
        }
        60 => device.texture_factor = value,
        41 => {
            if value > 1 {
                return Ok(INVALID);
            }
            device.color_key = value != 0;
        }
        _ => return Ok(device.render.set(key, value, true)),
    }
    Ok(0)
}
fn matrix_transfer<M: GuestMemory>(
    process: &mut Process<M>,
    device: &mut Device,
    key: u32,
    address: u32,
    set: bool,
) -> Result<u32> {
    if !(1..=6).contains(&key) || address == 0 {
        return Ok(INVALID);
    }
    let matrix = &mut device.matrices[(key - 1) as usize];
    let mut bytes = [0; 64];
    if set {
        process.read(address, &mut bytes)?;
        let values = std::array::from_fn::<_, 16, _>(|i| {
            f32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap()) as f64
        });
        if values.iter().any(|value| !value.is_finite()) {
            return Ok(INVALID);
        }
        *matrix = values;
    } else {
        for (slot, value) in bytes.as_chunks_mut::<4>().0.iter_mut().zip(matrix.iter()) {
            slot.copy_from_slice(&(*value as f32).to_le_bytes());
        }
        process.write(address, &bytes)?;
    }
    Ok(0)
}
fn set_clip_status<M: GuestMemory>(
    process: &Process<M>,
    device: &mut Device,
    address: u32,
) -> Result<u32> {
    if address == 0 {
        return Ok(INVALID);
    }
    let mut bytes = [0; 32];
    process.read(address, &mut bytes)?;
    let flags = resources::word(&bytes, 0);
    if flags & !3 != 0 {
        return Ok(NOT_IMPLEMENTED);
    }
    let bounds = std::array::from_fn(|i| resources::float(&bytes, 8 + i * 4));
    if flags & 2 != 0 && bounds.iter().any(|value| !value.is_finite()) {
        return Ok(INVALID);
    }
    if flags & 1 != 0 {
        device.clip_status = resources::word(&bytes, 4);
    }
    if flags & 2 != 0 {
        device.clip_extents = Some(bounds);
    }
    Ok(0)
}
