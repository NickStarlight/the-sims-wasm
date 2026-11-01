use super::*;
use crate::SparseMemory;
pub(super) struct Fixture {
    pub(super) graphics: Graphics,
    pub(super) process: Process<SparseMemory>,
    pub(super) modules: ModuleCatalog,
    pub(super) device: u32,
    pub(super) scratch: u32,
}
impl Fixture {
    pub(super) fn new() -> Self {
        let bindings: Vec<_> = crate::kernel::manifest()
            .iter()
            .filter(|a| a.dll == "ddraw.dll")
            .map(|a| serde_json::json!({"dll":a.dll,"name":a.name,"address":0x800000+a.id*4}))
            .collect();
        let config = serde_json::json!({"images":[{"base":0x400000,"sizeOfImage":4096,"isDll":false}],"nativeDlls":["ddraw.dll"],"nativeBindings":bindings});
        let modules = ModuleCatalog::from_json(&config.to_string()).unwrap();
        let mut process = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
        let scratch = process.heap_alloc(4096).unwrap();
        let mut graphics = Graphics::default();
        assert_eq!(
            graphics
                .call(&mut process, &modules, "DirectDrawCreate", &[0, scratch, 0])
                .unwrap(),
            0
        );
        let device = read_u32(&process, scratch).unwrap();
        Self {
            graphics,
            process,
            modules,
            device,
            scratch,
        }
    }
    pub(super) fn call(&mut self, name: &str, args: &[u32]) -> u32 {
        self.graphics
            .call(&mut self.process, &self.modules, name, args)
            .unwrap()
    }
    pub(super) fn word(&mut self, offset: u32, value: u32) {
        write_u32(&mut self.process, self.scratch + offset, value).unwrap();
    }
    pub(super) fn surface(&mut self, bits: u32, caps: u32) -> u32 {
        self.process.write(self.scratch, &[0; 108]).unwrap();
        for (o, v) in [
            (0, 108),
            (4, 0x1027),
            (8, 2),
            (12, 4),
            (20, 1),
            (72, 32),
            (76, if bits == 8 { 0x60 } else { 0x40 }),
            (84, bits),
            (104, caps),
        ] {
            self.word(o, v);
        }
        assert_eq!(
            self.call(
                "DDraw_CreateSurface",
                &[self.device, self.scratch, self.scratch + 128, 0]
            ),
            0
        );
        read_u32(&self.process, self.scratch + 128).unwrap()
    }
    pub(super) fn lock(&mut self, id: u32) -> u32 {
        self.word(256, 108);
        assert_eq!(
            self.call("Surface_Lock", &[id, 0, self.scratch + 256, 0, 0]),
            0
        );
        read_u32(&self.process, self.scratch + 292).unwrap()
    }
}
#[test]
fn guest_vtables_lock_pixels_and_palette_present_owned_rgba() {
    let mut f = Fixture::new();
    let table = read_u32(&f.process, f.device).unwrap();
    let api = crate::kernel::resolve("ddraw.dll", "DDraw_CreateSurface").unwrap();
    assert_eq!(
        read_u32(&f.process, table + 24).unwrap(),
        0x800000 + api.id * 4
    );
    let surface = f.surface(8, 0x200);
    let pixels = f.lock(surface);
    f.process.write(pixels, &[1, 2, 1, 2, 2, 1, 2, 1]).unwrap();
    assert_eq!(
        f.call("Surface_Lock", &[surface, 0, f.scratch + 256, 0, 0]),
        BUSY
    );
    assert_eq!(f.call("Surface_Unlock", &[surface, 0]), 0);
    f.process.write(f.scratch + 1024, &[0; 1024]).unwrap();
    f.process
        .write(f.scratch + 1028, &[255, 0, 0, 0, 0, 255, 0, 0])
        .unwrap();
    assert_eq!(
        f.call(
            "DDraw_CreatePalette",
            &[f.device, 4, f.scratch + 1024, f.scratch + 128, 0]
        ),
        0
    );
    let palette = read_u32(&f.process, f.scratch + 128).unwrap();
    assert_eq!(f.call("Surface_SetPalette", &[surface, palette]), 0);
    let frame = f.graphics.take_frame().unwrap();
    assert_eq!((frame.width, frame.height), (4, 2));
    assert_eq!(&frame.pixels[..8], &[255, 0, 0, 255, 0, 255, 0, 255]);
    assert!(f.graphics.take_frame().is_none());
    assert_eq!(f.call("Palette_Release", &[palette]), 1);
    assert_eq!(f.call("Surface_Release", &[surface]), 0);
    assert!(!f.graphics.palettes.contains_key(&palette));
    assert!(f.process.heap_size(pixels).is_err());
}
#[test]
fn self_blits_snapshot_overlap_and_color_keys_preserve_destination() {
    let mut f = Fixture::new();
    let surface = f.surface(8, 0);
    let pixels = f.lock(surface);
    f.process.write(pixels, &[1, 2, 3, 4, 0, 0, 0, 0]).unwrap();
    f.call("Surface_Unlock", &[surface, 0]);
    for (o, v) in [(512, 0), (516, 0), (520, 3), (524, 1)] {
        f.word(o, v);
    }
    assert_eq!(
        f.call(
            "Surface_BltFast",
            &[surface, 1, 0, surface, f.scratch + 512, 0]
        ),
        0
    );
    assert_eq!(
        &*f.graphics.surfaces[&surface].pixels,
        &[1, 1, 2, 3, 0, 0, 0, 0]
    );
    f.word(544, 1);
    f.word(548, 1);
    f.call("Surface_SetColorKey", &[surface, 8, f.scratch + 544]);
    assert_eq!(
        f.call(
            "Surface_BltFast",
            &[surface, 0, 1, surface, f.scratch + 512, 1]
        ),
        0
    );
    assert_eq!(
        &*f.graphics.surfaces[&surface].pixels,
        &[1, 1, 2, 3, 0, 0, 2, 0]
    );
}
#[test]
fn flip_moves_back_pixels_and_attachment_retains_back_lifetime() {
    let mut f = Fixture::new();
    let front = f.surface(16, 0x210);
    f.word(512, 4);
    assert_eq!(
        f.call(
            "Surface_GetAttachedSurface",
            &[front, f.scratch + 512, f.scratch + 128]
        ),
        0
    );
    let back = read_u32(&f.process, f.scratch + 128).unwrap();
    let pixels = f.lock(back);
    f.process.write(pixels, &[0, 0xf8]).unwrap();
    f.call("Surface_Unlock", &[back, 0]);
    assert!(f.graphics.take_frame().is_none());
    assert_eq!(f.call("Surface_Flip", &[front, 0, 0]), 0);
    assert_eq!(
        &f.graphics.take_frame().unwrap().pixels[..4],
        &[255, 0, 0, 255]
    );
    assert_eq!(f.call("Surface_Release", &[front]), 0);
    assert!(f.graphics.surfaces.contains_key(&back));
    assert_eq!(f.call("Surface_Release", &[back]), 0);
}
#[test]
fn enumeration_records_are_distinct_and_unknown_interfaces_fail() {
    let mut f = Fixture::new();
    let calls = f
        .graphics
        .enumerate(
            &mut f.process,
            "DDraw_EnumDisplayModes",
            &[f.device, 0, 0, 42, 0x500000],
        )
        .unwrap();
    assert_eq!(calls.len(), 3);
    assert_ne!(calls[0].1[0], calls[1].1[0]);
    assert_eq!(read_u32(&f.process, calls[0].1[0] + 12).unwrap(), 640);
    assert_eq!(calls[0].1[1], 42);
    f.process.write(f.scratch + 512, &[0xff; 16]).unwrap();
    assert_eq!(
        f.call(
            "DDraw_QueryInterface",
            &[f.device, f.scratch + 512, f.scratch + 128]
        ),
        NO_INTERFACE
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), 0);
}

#[test]
fn clipper_query_interface_uses_the_directdraw_iid_and_retains_object() {
    let mut f = Fixture::new();
    assert_eq!(
        f.call("DDraw_CreateClipper", &[f.device, 0, f.scratch + 128, 0]),
        0
    );
    let clipper = read_u32(&f.process, f.scratch + 128).unwrap();
    for (offset, value) in [
        (512, 0x6c14db85),
        (516, 0x11cea733),
        (520, 0x200021a5),
        (524, 0x60e50baf),
    ] {
        f.word(offset, value);
    }
    assert_eq!(
        f.call(
            "Clipper_QueryInterface",
            &[clipper, f.scratch + 512, f.scratch + 128]
        ),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), clipper);
    assert_eq!(f.call("Clipper_Release", &[clipper]), 1);
}

#[test]
fn blit_flags_preserve_legacy_masking_and_fast_validation_returns() {
    let mut f = Fixture::new();
    let surface = f.surface(8, 0);
    assert_eq!(
        f.call("Surface_Blt", &[surface, 0, surface, 0, 0x08000000, 0]),
        0
    );
    assert_eq!(
        f.call("Surface_BltFast", &[surface, 0, 0, surface, 0, 0x20]),
        0
    );
    assert_eq!(
        f.call("Surface_Blt", &[surface, 0, surface, 0, 0x00800000, 0]),
        0
    );
    assert_eq!(
        f.call("Surface_BltFast", &[surface, 0, 0, surface, 0, 1]),
        0
    );
}

#[test]
fn extreme_clipped_rectangles_use_full_unsigned_extent_without_index_overflow() {
    let mut f = Fixture::new();
    let surface = f.surface(8, 0);
    let pixels = f.lock(surface);
    f.process.write(pixels, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    f.call("Surface_Unlock", &[surface, 0]);
    for (offset, value) in [
        (512, i32::MIN as u32),
        (516, 0),
        (520, i32::MAX as u32),
        (524, 1),
    ] {
        f.word(offset, value);
    }
    assert_eq!(
        f.call("Surface_Blt", &[surface, f.scratch + 512, surface, 0, 0, 0]),
        0
    );
    assert_eq!(&f.graphics.surfaces[&surface].pixels[..4], &[1, 2, 3, 4]);
}

#[test]
fn legacy_duplicate_surface_has_independent_zeroed_storage_and_copied_keys() {
    let mut f = Fixture::new();
    let source = f.surface(8, 0);
    let pixels = f.lock(source);
    f.process.write(pixels, &[1, 2, 3, 4]).unwrap();
    f.call("Surface_Unlock", &[source, 0]);
    f.word(512, 3);
    f.word(516, 3);
    f.call("Surface_SetColorKey", &[source, 8, f.scratch + 512]);
    assert_eq!(
        f.call(
            "DDraw_DuplicateSurface",
            &[f.device, source, f.scratch + 128]
        ),
        0
    );
    let copy = read_u32(&f.process, f.scratch + 128).unwrap();
    assert_ne!(source, copy);
    assert_eq!(&f.graphics.surfaces[&copy].pixels[..4], &[0; 4]);
    assert_eq!(
        f.call("Surface_GetColorKey", &[copy, 8, f.scratch + 544]),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 544).unwrap(), 3);
    let pointer = f.lock(copy);
    f.process.write(pointer, &[7]).unwrap();
    f.call("Surface_Unlock", &[copy, 0]);
    assert_eq!(f.graphics.surfaces[&source].pixels[0], 1);
}

#[test]
fn legacy_clipper_snapshot_initialize_and_window_rules_are_preserved() {
    let mut f = Fixture::new();
    f.call("DDraw_CreateClipper", &[f.device, 0, f.scratch + 128, 0]);
    let clipper = read_u32(&f.process, f.scratch + 128).unwrap();
    assert_eq!(
        f.call("Clipper_GetClipList", &[clipper, 0, 0, f.scratch + 128]),
        0x887600a0
    );
    f.process.write(f.scratch + 512, &[0; 48]).unwrap();
    f.word(512, 32);
    f.word(524, 16);
    f.word(544, 123);
    assert_eq!(
        f.call("Clipper_SetClipList", &[clipper, f.scratch + 512, 0]),
        0
    );
    f.word(544, 999);
    assert_eq!(
        f.call(
            "Clipper_GetClipList",
            &[clipper, 0, f.scratch + 640, f.scratch + 128]
        ),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), 48);
    assert_eq!(read_u32(&f.process, f.scratch + 672).unwrap(), 123);
    assert_eq!(f.call("Clipper_Initialize", &[clipper, f.device, 0]), 0);
    assert_eq!(
        f.call("Clipper_Initialize", &[clipper, f.device, 0]),
        0x88760005
    );
    f.call("Clipper_SetHWnd", &[clipper, 0, 0x7600000]);
    assert_eq!(f.call("Clipper_SetClipList", &[clipper, 0, 0]), 0x88760088);
}

#[test]
fn legacy_depth_descriptor_attachment_and_detachment_keep_owned_references() {
    let mut f = Fixture::new();
    let surface = f.surface(16, 0);
    f.process.write(f.scratch, &[0; 108]).unwrap();
    for (o, v) in [
        (0, 108),
        (4, 0x47),
        (8, 2),
        (12, 4),
        (24, 16),
        (104, 0x20000),
    ] {
        f.word(o, v);
    }
    assert_eq!(
        f.call(
            "DDraw_CreateSurface",
            &[f.device, f.scratch, f.scratch + 128, 0]
        ),
        0
    );
    let depth = read_u32(&f.process, f.scratch + 128).unwrap();
    assert_eq!(f.call("Surface_AddAttachedSurface", &[surface, depth]), 0);
    assert_eq!(
        f.call("Surface_AddAttachedSurface", &[surface, depth]),
        INVALID
    );
    f.word(512, 0x20000);
    assert_eq!(
        f.call(
            "Surface_GetAttachedSurface",
            &[surface, f.scratch + 512, f.scratch + 128]
        ),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), depth);
    assert_eq!(
        f.call("Surface_GetPixelFormat", &[depth, f.scratch + 544]),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 548).unwrap(), 0x400);
    assert_eq!(
        f.call("Surface_DeleteAttachedSurface", &[surface, 0, depth]),
        0
    );
    assert_eq!(
        f.call("Surface_DeleteAttachedSurface", &[surface, 0, depth]),
        0x887600ff
    );
    assert_eq!(
        f.call(
            "Surface_GetAttachedSurface",
            &[surface, f.scratch + 512, f.scratch + 128]
        ),
        0x887600ff
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), 0);
}

#[test]
fn legacy_pixel_copy_clips_without_stretch_and_accepts_supported_rops() {
    let mut f = Fixture::new();
    let source = f.surface(8, 0);
    let dest = f.surface(8, 0);
    let pixels = f.lock(source);
    f.process.write(pixels, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    f.call("Surface_Unlock", &[source, 0]);
    for (o, v) in [(512, 0), (516, 0), (520, 2), (524, 1)] {
        f.word(o, v);
    }
    assert_eq!(
        f.call("Surface_Blt", &[dest, 0, source, f.scratch + 512, 0, 0]),
        0
    );
    assert_eq!(
        &*f.graphics.surfaces[&dest].pixels,
        &[1, 2, 0, 0, 0, 0, 0, 0]
    );
    f.process.write(f.scratch + 640, &[0; 100]).unwrap();
    f.word(648, 0xff0662);
    assert_eq!(
        f.call(
            "Surface_Blt",
            &[dest, 0, source, 0, 0x20000, f.scratch + 640]
        ),
        0
    );
    assert_eq!(&*f.graphics.surfaces[&dest].pixels, &[255; 8]);
    assert_eq!(read_u32(&f.process, f.scratch + 720).unwrap(), u32::MAX);
    f.word(648, 0x123456);
    assert_eq!(
        f.call(
            "Surface_Blt",
            &[dest, 0, source, 0, 0x20000, f.scratch + 640]
        ),
        0x88760118
    );
}

#[test]
fn legacy_lock_descriptor_and_batch_copy_preserve_full_surface_contract() {
    let mut f = Fixture::new();
    let source = f.surface(8, 0);
    let dest = f.surface(8, 0);
    for (o, v) in [(512, 1), (516, 0), (520, 2), (524, 1), (256, 108)] {
        f.word(o, v);
    }
    assert_eq!(
        f.call(
            "Surface_Lock",
            &[source, f.scratch + 512, f.scratch + 256, 0, 0]
        ),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 268).unwrap(), 4);
    let pointer = read_u32(&f.process, f.scratch + 292).unwrap();
    assert_eq!(pointer, f.graphics.surfaces[&source].guest_pixels);
    f.process.write(pointer, &[1, 2, 3, 4, 5, 6, 7, 8]).unwrap();
    f.call("Surface_Unlock", &[source, 0]);
    for (o, v) in [(640, 0), (644, source), (648, 0), (652, 0), (656, 0)] {
        f.word(o, v);
    }
    assert_eq!(
        f.call("Surface_BltBatch", &[dest, f.scratch + 640, 1, 0]),
        0
    );
    assert_eq!(
        &*f.graphics.surfaces[&dest].pixels,
        &[1, 2, 3, 4, 5, 6, 7, 8]
    );
    f.word(544, 2);
    f.word(548, 2);
    f.call("Surface_SetColorKey", &[source, 16, f.scratch + 544]);
    f.word(256, 108);
    f.call("Surface_GetSurfaceDesc", &[source, f.scratch + 256]);
    assert_ne!(read_u32(&f.process, f.scratch + 260).unwrap() & 0x8000, 0);
    assert_eq!(read_u32(&f.process, f.scratch + 312).unwrap(), 2);
}

#[test]
fn surface_dc_lease_is_owned_cached_and_holds_surface_until_release() {
    let mut f = Fixture::new();
    let mut user = crate::user::User::default();
    let surface = f.surface(16, 0);
    assert_eq!(
        f.graphics
            .surface_dc(
                &mut f.process,
                &mut user,
                "Surface_GetDC",
                &[surface, f.scratch + 128],
                1
            )
            .unwrap(),
        0
    );
    let dc = read_u32(&f.process, f.scratch + 128).unwrap();
    assert_ne!(dc, 0x20000);
    assert_eq!(user.dc_call("GetDeviceCaps", &[dc, 12], 1).unwrap(), 16);
    f.graphics
        .surface_dc(
            &mut f.process,
            &mut user,
            "Surface_GetDC",
            &[surface, f.scratch + 128],
            1,
        )
        .unwrap();
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), dc);
    assert_eq!(user.dc_call("ReleaseDC", &[0, dc], 1).unwrap(), 0);
    assert_eq!(
        f.graphics
            .surface_dc(
                &mut f.process,
                &mut user,
                "Surface_ReleaseDC",
                &[surface, dc],
                2
            )
            .unwrap(),
        INVALID
    );
    assert_eq!(f.call("Surface_Release", &[surface]), 1);
    assert_eq!(
        f.graphics
            .surface_dc(
                &mut f.process,
                &mut user,
                "Surface_ReleaseDC",
                &[surface, dc],
                1
            )
            .unwrap(),
        0
    );
    assert!(!f.graphics.surfaces.contains_key(&surface));
    assert_eq!(user.dc_call("GetDeviceCaps", &[dc, 12], 1).unwrap(), 0);
}

#[test]
fn legacy_palette_sizes_initialization_and_optional_entries_are_preserved() {
    let mut f = Fixture::new();
    for (flags, count) in [(0x100, 2), (0x200, 4), (1, 16), (4, 256)] {
        assert_eq!(
            f.call(
                "DDraw_CreatePalette",
                &[f.device, flags, 0, f.scratch + 128, 0]
            ),
            0
        );
        let palette = read_u32(&f.process, f.scratch + 128).unwrap();
        assert_eq!(f.graphics.palettes[&palette].count, count as usize);
        assert_eq!(
            f.call("Palette_Initialize", &[palette, f.device, flags, 0]),
            0x88760005
        );
        f.process.write(f.scratch + 512, &[11, 22, 33, 44]).unwrap();
        assert_eq!(
            f.call(
                "Palette_SetEntries",
                &[palette, 123, count - 1, 1, f.scratch + 512]
            ),
            0
        );
        assert_eq!(
            f.call(
                "Palette_GetEntries",
                &[palette, 123, count - 1, 1, f.scratch + 544]
            ),
            0
        );
        let mut entry = [0; 4];
        f.process.read(f.scratch + 544, &mut entry).unwrap();
        assert_eq!(entry, [11, 22, 33, 44]);
        assert_eq!(
            f.call(
                "Palette_GetEntries",
                &[palette, 0, count, 0, f.scratch + 544]
            ),
            INVALID
        );
        f.call("Palette_Release", &[palette]);
    }
}

#[test]
fn legacy_device_output_slots_and_surface_parent_interface_are_bound() {
    let mut f = Fixture::new();
    let surface = f.surface(16, 0);
    assert_eq!(
        f.call("Surface_GetDDInterface", &[surface, f.scratch + 128]),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), f.device);
    for name in [
        "DDraw_GetFourCCCodes",
        "DDraw_GetGDISurface",
        "DDraw_GetScanLine",
    ] {
        f.word(128, 123);
        assert_eq!(f.call(name, &[f.device, f.scratch + 128, 0]), 0);
        assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), 0);
    }
    f.word(128, 123);
    assert_eq!(
        f.call("DDraw_GetSurfaceFromDC", &[f.device, 0, f.scratch + 128]),
        0
    );
    assert_eq!(read_u32(&f.process, f.scratch + 128).unwrap(), 0);
}

#[test]
fn present_rgb565_respects_row_pitch_and_matches_generic_conversion() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0);
    let surface = f.graphics.surfaces.get_mut(&id).unwrap();
    surface.primary = true;
    surface.pitch += 4;
    surface.pixels = Rc::new(
        (0..surface.pitch * surface.height)
            .map(|i| (i * 73 + 19) as u8)
            .collect(),
    );
    let mut expected = Vec::new();
    for y in 0..surface.height {
        for x in 0..surface.width {
            expected.extend(surface.format.rgba(format::read_pixel(surface, x, y), None));
        }
    }
    f.graphics.present(id).unwrap();
    let frame = f.graphics.take_frame().unwrap();
    assert_eq!((frame.width, frame.height), (4, 2));
    assert_eq!(frame.pixels, expected);
}

#[test]
fn pending_frames_replace_pixels_without_mutating_frames_already_taken() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0);
    f.graphics.surfaces.get_mut(&id).unwrap().primary = true;
    f.graphics.present(id).unwrap();
    let held = f.graphics.take_frame().unwrap();
    for value in [0xf800u16, 0x07e0, 0x001f] {
        let surface = f.graphics.surfaces.get_mut(&id).unwrap();
        for pixel in Rc::make_mut(&mut surface.pixels).as_chunks_mut::<2>().0 {
            *pixel = value.to_le_bytes();
        }
        f.graphics.present(id).unwrap();
    }
    let latest = f.graphics.take_frame().unwrap();
    assert!(held
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [0, 0, 0, 255]));
    assert!(latest
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [0, 0, 255, 255]));
    assert!(f.graphics.take_frame().is_none());
}

#[test]
fn pending_frame_defers_conversion_and_retains_pixels_after_surface_changes() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0);
    f.graphics.surfaces.get_mut(&id).unwrap().primary = true;
    f.graphics.present(id).unwrap();
    assert!(f.graphics.frame.as_ref().unwrap().pixels.is_empty());
    // Mutation without a new presentation cannot change the completed image.
    f.graphics
        .surfaces
        .get_mut(&id)
        .unwrap()
        .pixels_mut()
        .fill(255);
    f.graphics.surfaces.remove(&id);
    let frame = f.graphics.take_frame().unwrap();
    assert!(frame
        .pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| *p == [0, 0, 0, 255]));
}

#[test]
fn packed_rgb565_retains_original_pitched_pixels_without_an_extra_copy() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0);
    let surface = f.graphics.surfaces.get_mut(&id).unwrap();
    surface.primary = true;
    surface.pitch += 4;
    surface.pixels = Rc::new(
        (0..surface.pitch * surface.height)
            .map(|i| i as u8)
            .collect(),
    );
    let original = surface.pixels.clone();
    f.graphics.present(id).unwrap();
    let packed = f.graphics.take_packed_frame().unwrap();
    assert_eq!(
        (packed.width, packed.height, packed.pitch, packed.format),
        (4, 2, 12, 16)
    );
    assert!(Rc::ptr_eq(&packed.pixels, &original));
    assert!(f.graphics.take_packed_frame().is_none());
    assert!(f.graphics.take_frame().is_none());
    f.graphics
        .surfaces
        .get_mut(&id)
        .unwrap()
        .pixels_mut()
        .fill(255);
    f.graphics.present(id).unwrap();
    f.graphics.surfaces.remove(&id);
    let latest = f.graphics.take_packed_frame().unwrap();
    assert!(latest.pixels.iter().all(|&byte| byte == 255));
    assert_eq!(packed.pixels.as_slice(), (0..24).collect::<Vec<u8>>());
}

#[test]
fn packed_other_formats_use_the_existing_rgba_conversion() {
    for bits in [8, 15, 24, 32] {
        let mut f = Fixture::new();
        let id = f.surface(bits, 0);
        let surface = f.graphics.surfaces.get_mut(&id).unwrap();
        surface.primary = true;
        surface.pitch += 4;
        surface.pixels = Rc::new(
            (0..surface.pitch * surface.height)
                .map(|i| (i * 17) as u8)
                .collect(),
        );
        if bits == 8 {
            surface.palette = 1;
            f.graphics.palettes.insert(
                1,
                Palette {
                    flags: 4,
                    count: 256,
                    entries: std::array::from_fn(|i| [i as u8, 255 - i as u8, 71, 0]),
                },
            );
        }
        f.graphics.present(id).unwrap();
        let expected = f.graphics.take_frame().unwrap();
        f.graphics.present(id).unwrap();
        // The snapshot includes the old palette, independent of subsequent palette edits.
        f.graphics.palettes.clear();
        let actual = f.graphics.take_packed_frame().unwrap();
        assert_eq!(
            (actual.width, actual.height, actual.pitch, actual.format),
            (4, 2, 16, 32)
        );
        assert_eq!(actual.pixels.as_slice(), expected.pixels);
    }
}

#[test]
fn repeated_locks_preserve_guest_writes_and_refresh_after_blits_and_flips() {
    let mut f = Fixture::new();
    let front = f.surface(16, 0x210);
    let back = f.graphics.surfaces[&front].back;
    let red = [0, 248].repeat(8);
    let green = [224, 7].repeat(8);
    for (id, data) in [(front, &red), (back, &green)] {
        let pointer = f.lock(id);
        f.process.write(pointer, data).unwrap();
        assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
        assert!(f.graphics.surfaces[&id].guest_pixels_current);
        assert_eq!(f.lock(id), pointer);
        let mut actual = vec![0; data.len()];
        f.process.read(pointer, &mut actual).unwrap();
        assert_eq!(&actual, data);
        assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
    }
    assert_eq!(f.call("Surface_Flip", &[front, 0, 0]), 0);
    for (id, data) in [(front, &green), (back, &red)] {
        assert!(!f.graphics.surfaces[&id].guest_pixels_current);
        let pointer = f.lock(id);
        let mut actual = vec![0; data.len()];
        f.process.read(pointer, &mut actual).unwrap();
        assert_eq!(&actual, data);
        assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
    }
    assert_eq!(f.call("Surface_BltFast", &[back, 0, 0, front, 0, 0]), 0);
    assert!(!f.graphics.surfaces[&back].guest_pixels_current);
    let pointer = f.lock(back);
    let mut actual = vec![0; green.len()];
    f.process.read(pointer, &mut actual).unwrap();
    assert_eq!(actual, green);
    // Even a cached lock must read changed guest data back on Unlock.
    f.process.write(pointer, &red).unwrap();
    assert_eq!(f.call("Surface_Unlock", &[back, 0]), 0);
    assert_eq!(*f.graphics.surfaces[&back].pixels, red);
}
