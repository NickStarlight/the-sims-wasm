//! Guest COM vtables contain installed native thunk addresses, never Rust pointers.
use super::*;
const UNKNOWN: [u32; 4] = [0, 0, 0xc0, 0x46000000];
pub(super) const DEVICE_IDS: [(u32, [u32; 4]); 4] = [
    (1, [0x6c14db80, 0x11cea733, 0x200021a5, 0x60e50baf]),
    (2, [0xb3a6f3e0, 0x11cf2b43, 0xaa00dea2, 0x5633b900]),
    (4, [0x9c59509a, 0x11d139bd, 0xc0004a8c, 0xc530d94f]),
    (7, [0x15e65ec0, 0x11d23b9c, 0x60002fb9, 0x5bea9797]),
];
const SURFACE_IDS: [[u32; 4]; 5] = [
    [0x6c14db81, 0x11cea733, 0x200021a5, 0x60e50baf],
    [0x57805885, 0x11cf6eec, 0x23a84194, 0x270ec103],
    [0xda044e00, 0x11d069b2, 0xaa00d5a1, 0xbbdfb800],
    [0x0b2b8630, 0x11d0ad35, 0x6000a68e, 0x5bea9797],
    [0x06675a80, 0x11d23b9b, 0x60002fb9, 0x5bea9797],
];
impl Graphics {
    pub(super) fn object<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        kind: Kind,
    ) -> Result<u32> {
        let (key, length, methods) = vtable(kind);
        let table = if let Some(&table) = self.vtables.get(&key) {
            table
        } else {
            let module = modules
                .handle(Some("ddraw.dll"))
                .ok_or_else(|| Error::Host("DirectDraw native module is not installed".into()))?;
            let mut bytes = vec![0; length * 4];
            for (slot, name) in methods {
                let address = modules
                    .procedure(module, Symbol::Name(name.into()))
                    .map_err(|_| Error::Host(format!("missing DirectDraw thunk {name}")))?;
                bytes[slot * 4..slot * 4 + 4].copy_from_slice(&address.to_le_bytes());
            }
            let table = process.heap_alloc(bytes.len() as u32)?;
            process.write(table, &bytes)?;
            self.vtables.insert(key, table);
            table
        };
        let object = process.heap_alloc(4)?;
        write_u32(process, object, table)?;
        self.objects.insert(
            object,
            Object {
                references: 1,
                kind,
            },
        );
        Ok(object)
    }
    pub(super) fn device<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        version: u32,
    ) -> Result<u32> {
        if let Some(&id) = self.devices.get(&version) {
            self.add_ref(id);
            return Ok(id);
        }
        let id = self.object(process, modules, Kind::Device(version))?;
        self.devices.insert(version, id);
        Ok(id)
    }
    pub(super) fn add_ref(&mut self, id: u32) -> u32 {
        self.objects.get_mut(&id).map_or(0, |object| {
            object.references = object.references.saturating_add(1);
            object.references
        })
    }
    pub(super) fn release<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        id: u32,
    ) -> Result<u32> {
        let Some(object) = self.objects.get_mut(&id) else {
            return Ok(0);
        };
        object.references -= 1;
        if object.references != 0 {
            return Ok(object.references);
        }
        let kind = object.kind;
        self.objects.remove(&id);
        match kind {
            Kind::Device(version) => {
                self.devices.remove(&version);
            }
            Kind::Surface => {
                if let Some(surface) = self.surfaces.remove(&id) {
                    if surface.guest_pixels != 0 {
                        process.heap_free(surface.guest_pixels)?;
                    }
                    for child in [
                        surface.back,
                        surface.depth,
                        surface.palette,
                        surface.clipper,
                    ] {
                        if child != 0 {
                            self.release(process, child)?;
                        }
                    }
                    if surface.back != 0 {
                        if let Some(back) = self.surfaces.get_mut(&surface.back) {
                            back.front = 0;
                        }
                    }
                }
            }
            Kind::Palette => {
                self.palettes.remove(&id);
            }
            Kind::D3d(_) => self.d3d_release(process, id)?,
            Kind::Clipper => {
                self.clippers.remove(&id);
            }
        }
        process.heap_free(id)?;
        Ok(0)
    }
    pub(super) fn query<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        modules: &ModuleCatalog,
        args: &[u32],
    ) -> Result<u32> {
        if args[1] == 0 || args[2] == 0 {
            return Ok(INVALID);
        }
        let Some(object) = self.objects.get(&args[0]) else {
            return Ok(INVALID);
        };
        let kind = object.kind;
        write_u32(process, args[2], 0)?;
        let mut raw = [0; 16];
        process.read(args[1], &mut raw)?;
        let iid =
            std::array::from_fn(|i| u32::from_le_bytes(raw[i * 4..i * 4 + 4].try_into().unwrap()));
        if let Some(output) = self.d3d_query(process, modules, args[0], iid)? {
            write_u32(process, args[2], output)?;
            return Ok(0);
        }
        let wanted = match kind {
            Kind::Device(_) => {
                if iid == UNKNOWN {
                    Some(1)
                } else {
                    DEVICE_IDS
                        .iter()
                        .find(|(_, guid)| *guid == iid)
                        .map(|(version, _)| *version)
                }
            }
            _ => None,
        };
        let output = if let Some(version) = wanted {
            self.device(process, modules, version)?
        } else {
            let supported = iid == UNKNOWN
                || match kind {
                    Kind::Surface => SURFACE_IDS.contains(&iid),
                    Kind::Palette => iid == [0x6c14db84, 0x11cea733, 0x200021a5, 0x60e50baf],
                    Kind::Clipper => iid == [0x6c14db85, 0x11cea733, 0x200021a5, 0x60e50baf],
                    Kind::Device(_) | Kind::D3d(_) => false,
                };
            if !supported {
                write_u32(process, args[2], 0)?;
                return Ok(NO_INTERFACE);
            }
            self.add_ref(args[0]);
            args[0]
        };
        write_u32(process, args[2], output)?;
        Ok(0)
    }
}
fn vtable(kind: Kind) -> (String, usize, Vec<(usize, &'static str)>) {
    match kind {
        Kind::D3d(interface) => super::d3d::interfaces::vtable(interface),
        Kind::Device(version) => (
            format!("device{version}"),
            30,
            vec![
                (0, "DDraw_QueryInterface"),
                (1, "DDraw_AddRef"),
                (2, "DDraw_Release"),
                (3, "DDraw_Compact"),
                (10, "DDraw_FlipToGDISurface"),
                (13, "DDraw_GetFourCCCodes"),
                (14, "DDraw_GetGDISurface"),
                (16, "DDraw_GetScanLine"),
                (18, "DDraw_Initialize"),
                (24, "DDraw_GetSurfaceFromDC"),
                (25, "DDraw_RestoreAllSurfaces"),
                (26, "DDraw_TestCooperativeLevel"),
                (27, "DDraw_GetDeviceIdentifier"),
                (4, "DDraw_CreateClipper"),
                (5, "DDraw_CreatePalette"),
                (6, "DDraw_CreateSurface"),
                (7, "DDraw_DuplicateSurface"),
                (8, "DDraw_EnumDisplayModes"),
                (11, "DDraw_GetCaps"),
                (12, "DDraw_GetDisplayMode"),
                (15, "DDraw_GetMonitorFrequency"),
                (17, "DDraw_GetVerticalBlankStatus"),
                (19, "DDraw_RestoreDisplayMode"),
                (20, "DDraw_SetCooperativeLevel"),
                (
                    21,
                    if version == 1 {
                        "DDraw_SetDisplayMode"
                    } else {
                        "DDraw_SetDisplayModeEx"
                    },
                ),
                (22, "DDraw_WaitForVerticalBlank"),
                (23, "DDraw_GetAvailableVidMem"),
            ],
        ),
        Kind::Surface => (
            "surface".into(),
            49,
            vec![
                (0, "Surface_QueryInterface"),
                (1, "Surface_AddRef"),
                (2, "Surface_Release"),
                (3, "Surface_AddAttachedSurface"),
                (8, "Surface_DeleteAttachedSurface"),
                (16, "Surface_GetColorKey"),
                (23, "Surface_Initialize"),
                (41, "Surface_GetPrivateData"),
                (43, "Surface_GetUniquenessValue"),
                (5, "Surface_Blt"),
                (6, "Surface_BltBatch"),
                (7, "Surface_BltFast"),
                (11, "Surface_Flip"),
                (12, "Surface_GetAttachedSurface"),
                (13, "Surface_GetBltStatus"),
                (14, "Surface_GetCaps"),
                (15, "Surface_GetClipper"),
                (17, "Surface_GetDC"),
                (26, "Surface_ReleaseDC"),
                (36, "Surface_GetDDInterface"),
                (18, "Surface_GetFlipStatus"),
                (20, "Surface_GetPalette"),
                (21, "Surface_GetPixelFormat"),
                (22, "Surface_GetSurfaceDesc"),
                (24, "Surface_IsLost"),
                (25, "Surface_Lock"),
                (27, "Surface_Restore"),
                (28, "Surface_SetClipper"),
                (29, "Surface_SetColorKey"),
                (31, "Surface_SetPalette"),
                (32, "Surface_Unlock"),
            ],
        ),
        Kind::Palette => (
            "palette".into(),
            7,
            vec![
                (0, "Palette_QueryInterface"),
                (1, "Palette_AddRef"),
                (2, "Palette_Release"),
                (3, "Palette_GetCaps"),
                (4, "Palette_GetEntries"),
                (5, "Palette_Initialize"),
                (6, "Palette_SetEntries"),
            ],
        ),
        Kind::Clipper => (
            "clipper".into(),
            9,
            vec![
                (0, "Clipper_QueryInterface"),
                (1, "Clipper_AddRef"),
                (2, "Clipper_Release"),
                (3, "Clipper_GetClipList"),
                (5, "Clipper_Initialize"),
                (7, "Clipper_SetClipList"),
                (4, "Clipper_GetHWnd"),
                (6, "Clipper_IsClipListChanged"),
                (8, "Clipper_SetHWnd"),
            ],
        ),
    }
}
