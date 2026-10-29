use super::*;
impl Graphics {
    pub(in crate::graphics) fn d3d_query<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        id: u32,
        iid: [u32; 4],
    ) -> Result<Option<u32>> {
        let kind = self.objects.get(&id).map(|o| o.kind);
        if matches!(
            kind,
            Some(Kind::Device(_) | Kind::D3d(Interface::Parent1 | Interface::Parent3))
        ) {
            if iid == interfaces::PARENT1 {
                return self.d3d_parent(p, modules, 1).map(Some);
            }
            if iid == interfaces::PARENT3 {
                return self.d3d_parent(p, modules, 3).map(Some);
            }
        }
        let device2 = [0x93281501, 0x11d08cf8, 0xa000ab89, 0x294105c9];
        let device3 = [0xb0ab3b60, 0x11d133d7, 0xc00081a9, 0x74b1d74f];
        if let Some(canonical) = self.d3d.device_id(id) {
            if iid == device3 {
                self.add_ref(canonical);
                return Ok(Some(canonical));
            }
            if iid == device2 {
                let existing = self
                    .d3d
                    .device_aliases
                    .iter()
                    .find(|(_, value)| **value == canonical)
                    .map(|(&key, _)| key);
                let view = if let Some(view) = existing {
                    self.add_ref(view);
                    view
                } else {
                    let view = self.object(p, modules, Kind::D3d(Interface::Device2))?;
                    self.add_ref(canonical);
                    self.d3d.device_aliases.insert(view, canonical);
                    view
                };
                return Ok(Some(view));
            }
        }
        let texture1 = [0x2cdcd9e0, 0x11cf25a0, 0xaa001aa3, 0x5633b900];
        let texture2 = [0x93281502, 0x11d08cf8, 0xa000ab89, 0x294105c9];
        if (iid == texture1 || iid == texture2)
            && (self.surfaces.contains_key(&id) || self.d3d.textures.contains_key(&id))
        {
            let surface = self.d3d.textures.get(&id).copied().unwrap_or(id);
            let wanted = if iid == texture1 {
                Interface::Texture1
            } else {
                Interface::Texture2
            };
            let existing = self
                .d3d
                .textures
                .iter()
                .find(|(key, value)| {
                    **value == surface
                        && self
                            .objects
                            .get(key)
                            .is_some_and(|o| o.kind == Kind::D3d(wanted))
                })
                .map(|(&key, _)| key);
            let view = if let Some(view) = existing {
                self.add_ref(view);
                view
            } else {
                let view = self.object(p, modules, Kind::D3d(wanted))?;
                self.add_ref(surface);
                self.d3d.textures.insert(view, surface);
                view
            };
            return Ok(Some(view));
        }
        let supported = match kind {
            Some(Kind::D3d(Interface::Viewport)) => [
                [0x4417c146, 0x11cf33ad, 0x00006f81, 0x6e1520c0],
                [0x93281500, 0x11d08cf8, 0xa000ab89, 0x294105c9],
                [0xb0ab3b61, 0x11d133d7, 0xc00081a9, 0x74b1d74f],
            ]
            .contains(&iid),
            Some(Kind::D3d(Interface::Light)) => {
                iid == [0x4417c142, 0x11cf33ad, 0x00006f81, 0x6e1520c0]
            }
            Some(Kind::D3d(Interface::Material1)) => {
                iid == [0x4417c144, 0x11cf33ad, 0x00006f81, 0x6e1520c0]
            }
            Some(Kind::D3d(Interface::Material3)) => {
                iid == [0xca9c46f4, 0x11d1d3c5, 0x60005ab7, 0x12b35208]
            }
            _ => false,
        };
        if supported {
            self.add_ref(id);
            Ok(Some(id))
        } else {
            Ok(None)
        }
    }
    pub(in crate::graphics) fn d3d_release<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        id: u32,
    ) -> Result<()> {
        self.d3d.parents.retain(|_, value| *value != id);
        self.d3d.materials.remove(&id);
        self.d3d.lights.remove(&id);
        if let Some(surface) = self.d3d.textures.remove(&id) {
            self.release(p, surface)?;
        }
        if let Some(device) = self.d3d.device_aliases.remove(&id) {
            self.release(p, device)?;
        }
        if let Some(viewport) = self.d3d.viewports.remove(&id) {
            for light in viewport.lights {
                if let Some(l) = self.d3d.lights.get_mut(&light) {
                    l.viewport = 0;
                }
                self.release(p, light)?;
            }
        }
        if let Some(device) = self.d3d.devices.remove(&id) {
            self.release(p, device.parent)?;
            self.release(p, device.target)?;
            for view in device.attached {
                if let Some(v) = self.d3d.viewports.get_mut(&view) {
                    v.device = 0;
                }
                self.release(p, view)?;
            }
        }
        Ok(())
    }
}
