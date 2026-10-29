use super::*;
impl Graphics {
    pub(super) fn d3d_parent<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        version: u32,
    ) -> Result<u32> {
        if let Some(&id) = self.d3d.parents.get(&version) {
            self.add_ref(id);
            return Ok(id);
        }
        let id = self.object(
            p,
            modules,
            Kind::D3d(if version == 1 {
                Interface::Parent1
            } else {
                Interface::Parent3
            }),
        )?;
        self.d3d.parents.insert(version, id);
        Ok(id)
    }
    pub(in crate::graphics) fn d3d_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        if name.starts_with("D3DTexture_") {
            return self.d3d_texture_call(p, name, a);
        }
        if name.starts_with("D3DDevice") {
            return self.d3d_device_call(p, modules, name, a);
        }
        if name.starts_with("D3DViewport_") {
            return self.d3d_viewport_call(p, name, a);
        }
        match name {
            "D3D_Initialize" => Ok(0x88760005),
            "D3D_FindDevice" => Ok(0x887600ff),
            "D3D_EvictManagedTextures" => Ok(0),
            "D3D_CreateLight" | "D3D_CreateMaterial" | "D3D_CreateViewport" => {
                if a[1] == 0 || a[2] != 0 {
                    return Ok(INVALID);
                }
                write_u32(p, a[1], 0)?;
                let kind = match name {
                    "D3D_CreateLight" => Interface::Light,
                    "D3D_CreateViewport" => Interface::Viewport,
                    _ => {
                        if matches!(
                            self.objects.get(&a[0]).map(|o| o.kind),
                            Some(Kind::D3d(Interface::Parent1))
                        ) {
                            Interface::Material1
                        } else {
                            Interface::Material3
                        }
                    }
                };
                let id = self.object(p, modules, Kind::D3d(kind))?;
                match kind {
                    Interface::Light => {
                        self.d3d.lights.insert(id, Light::default());
                    }
                    Interface::Viewport => {
                        self.d3d.viewports.insert(id, Viewport::default());
                    }
                    _ => {
                        self.d3d.next_material += 1;
                        self.d3d
                            .materials
                            .insert(id, Material::new(self.d3d.next_material));
                    }
                }
                write_u32(p, a[1], id)?;
                Ok(0)
            }
            "D3D_CreateDevice" => {
                if a[3] == 0 || a[4] != 0 {
                    return Ok(INVALID);
                }
                write_u32(p, a[3], 0)?;
                let mut guid = [0; 16];
                p.read(a[1], &mut guid)?;
                let wanted: Vec<u8> = interfaces::RGB
                    .iter()
                    .flat_map(|x| x.to_le_bytes())
                    .collect();
                if guid.as_slice() != wanted
                    || !self
                        .surfaces
                        .get(&a[2])
                        .is_some_and(|s| matches!(s.format.bits, 16 | 32))
                {
                    return Ok(INVALID);
                }
                let id = self.object(p, modules, Kind::D3d(Interface::Device3))?;
                self.add_ref(a[0]);
                self.add_ref(a[2]);
                self.d3d.devices.insert(id, device::Device::new(a[0], a[2]));
                write_u32(p, a[3], id)?;
                Ok(0)
            }
            "D3D_CreateVertexBuffer" => {
                if a[2] != 0 {
                    write_u32(p, a[2], 0)?;
                }
                Ok(NOT_IMPLEMENTED)
            }
            "D3DMaterial_SetMaterial" | "D3DMaterial_GetMaterial" => {
                let Some(m) = self.d3d.materials.get_mut(&a[0]) else {
                    return Ok(INVALID);
                };
                resources::transfer(p, &mut m.bytes, a[1], name.ends_with("SetMaterial"), false)
            }
            "D3DMaterial_GetHandle" => {
                if a[2] == 0 || self.d3d.device_id(a[1]).is_none() {
                    return Ok(INVALID);
                }
                let Some(m) = self.d3d.materials.get(&a[0]) else {
                    return Ok(INVALID);
                };
                write_u32(p, a[2], m.handle)?;
                Ok(0)
            }
            "D3DLight_SetLight" | "D3DLight_GetLight" => {
                let Some(l) = self.d3d.lights.get_mut(&a[0]) else {
                    return Ok(INVALID);
                };
                resources::transfer(p, &mut l.bytes, a[1], name.ends_with("SetLight"), true)
            }
            _ => Ok(NOT_IMPLEMENTED),
        }
    }
}
