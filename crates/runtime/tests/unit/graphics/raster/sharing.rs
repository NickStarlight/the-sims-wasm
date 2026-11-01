use super::super::tests::Fixture;
use super::*;

#[test]
fn unique_destinations_keep_reusable_storage() {
    let mut f = Fixture::new();
    let source_id = f.surface(16, 0);
    let dest_id = f.surface(16, 0);
    let source = f.graphics.surfaces.remove(&source_id).unwrap();
    let mut dest = f.graphics.surfaces.remove(&dest_id).unwrap();
    dest.pixels_mut().fill(0xff);
    let original_storage = Rc::as_ptr(&dest.pixels);
    let rect = Rect {
        left: 0,
        top: 0,
        right: 4,
        bottom: 2,
    };
    copy_pixels(
        &mut dest,
        Some(&source),
        (rect, rect),
        (4, 2),
        None,
        (None, None),
        None,
    );
    assert_eq!(*dest.pixels, *source.pixels);
    assert_eq!(Rc::as_ptr(&dest.pixels), original_storage);
    assert!(!Rc::ptr_eq(&dest.pixels, &source.pixels));
    let held = dest.pixels.clone();
    copy_pixels(
        &mut dest,
        Some(&source),
        (rect, rect),
        (4, 2),
        None,
        (None, None),
        None,
    );
    assert!(Rc::ptr_eq(&dest.pixels, &source.pixels));
    assert!(!Rc::ptr_eq(&dest.pixels, &held));
}

#[test]
fn full_copy_sharing_matches_pixels_and_retains_independent_snapshots() {
    let mut f = Fixture::new();
    for source_bits in [8, 15, 16, 24, 32] {
        for dest_bits in [8, 15, 16, 24, 32] {
            let source_id = f.surface(source_bits, 0);
            let dest_id = f.surface(dest_bits, 0);
            for padding in [0, 4] {
                let mut source = f.graphics.surfaces[&source_id].clone();
                let mut original = f.graphics.surfaces[&dest_id].clone();
                for (surface, salt) in [(&mut source, 19), (&mut original, 71)] {
                    surface.pitch += padding;
                    surface.pixels = Rc::new(
                        (0..surface.pitch * surface.height)
                            .map(|i| (i * 73 + salt) as u8)
                            .collect(),
                    );
                    surface.guest_pixels_current = true;
                }
                for crop in [false, true] {
                    let width = if crop { 3 } else { 4 };
                    let dr = Rect {
                        left: 0,
                        top: 0,
                        right: width as i32,
                        bottom: 2,
                    };
                    let sr = Rect {
                        left: if crop { 1 } else { 0 },
                        top: 0,
                        right: 4,
                        bottom: 2,
                    };
                    for keys in [(None, None), (Some(19), None), (None, Some(71))] {
                        for fill in [None, Some(0x12345678)] {
                            let mut expected = (*original.pixels).clone();
                            let bytes = original.format.bytes() as usize;
                            for y in 0..2 {
                                for x in 0..width {
                                    let old = format::read_pixel(&original, x, y);
                                    let value = format::read_pixel(&source, sr.left as u32 + x, y);
                                    if fill.is_none()
                                        && (keys.0 == Some(value)
                                            || keys.1.is_some_and(|key| old != key))
                                    {
                                        continue;
                                    }
                                    let value = fill.unwrap_or_else(|| {
                                        convert(
                                            value,
                                            source.format.bytes(),
                                            original.format.bytes(),
                                            None,
                                        )
                                    });
                                    let offset = (y * original.pitch) as usize + x as usize * bytes;
                                    expected[offset..offset + bytes]
                                        .copy_from_slice(&value.to_le_bytes()[..bytes]);
                                }
                            }
                            let mut actual = original.clone();
                            copy_pixels(
                                &mut actual,
                                Some(&source),
                                (dr, sr),
                                (width, 2),
                                fill,
                                keys,
                                None,
                            );
                            assert_eq!(*actual.pixels, expected,
                                "source={source_bits} dest={dest_bits} padding={padding} crop={crop} keys={keys:?} fill={fill:?}");
                            assert!(!actual.guest_pixels_current);
                            let shared = source.format == original.format
                                && padding == 0
                                && !crop
                                && fill.is_none()
                                && keys == (None, None);
                            assert_eq!(Rc::ptr_eq(&actual.pixels, &source.pixels), shared);
                            if shared {
                                let snapshot = actual.pixels.clone();
                                let mut changed_source = source.clone();
                                changed_source.pixels_mut()[0] ^= 0xff;
                                assert_eq!(*actual.pixels, expected);
                                actual.pixels_mut()[1] ^= 0xff;
                                assert_eq!(*snapshot, expected);
                                assert_eq!(source.pixels[1], expected[1]);
                            }
                            assert_eq!(source.pixels[0], 19);
                            assert_eq!(original.pixels[0], 71);
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn shared_blits_refresh_guest_locks_and_preserve_held_frames_and_source_writes() {
    let mut f = Fixture::new();
    let source = f.surface(16, 0);
    let dest = f.surface(16, 0);
    f.graphics.surfaces.get_mut(&dest).unwrap().primary = true;
    let old = [0, 248].repeat(8);
    let copied = [224, 7].repeat(8);
    for (id, data) in [(dest, &old), (source, &copied)] {
        let pointer = f.lock(id);
        f.process.write(pointer, data).unwrap();
        assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
    }
    let held_old = f.graphics.take_packed_frame().unwrap();
    let cached_pointer = f.graphics.surfaces[&dest].guest_pixels;
    assert_eq!(f.call("Surface_Blt", &[dest, 0, source, 0, 0, 0]), 0);
    assert!(!f.graphics.surfaces[&dest].guest_pixels_current);
    assert!(Rc::ptr_eq(
        &f.graphics.surfaces[&dest].pixels,
        &f.graphics.surfaces[&source].pixels
    ));
    let held_copy = f.graphics.take_packed_frame().unwrap();
    assert_eq!(f.lock(dest), cached_pointer);
    let mut bytes = vec![0; copied.len()];
    f.process.read(cached_pointer, &mut bytes).unwrap();
    assert_eq!(bytes, copied);
    f.process.write(cached_pointer, &[31, 0]).unwrap();
    assert_eq!(f.call("Surface_Unlock", &[dest, 0]), 0);
    assert_eq!(f.graphics.surfaces[&source].pixels.as_slice(), copied);
    assert_eq!(held_old.pixels.as_slice(), old);
    assert_eq!(held_copy.pixels.as_slice(), copied);

    // A locked source still uses its last completed runtime pixels, as before.
    let source_pointer = f.lock(source);
    f.process.write(source_pointer, &old).unwrap();
    assert_eq!(f.call("Surface_BltFast", &[dest, 0, 0, source, 0, 0]), 0);
    assert_eq!(f.graphics.surfaces[&dest].pixels.as_slice(), copied);
    assert_eq!(f.call("Surface_Unlock", &[source, 0]), 0);
    assert_eq!(f.graphics.surfaces[&source].pixels.as_slice(), old);
    assert_eq!(f.graphics.surfaces[&dest].pixels.as_slice(), copied);

    // A full self-blit preserves both the surface and retained completed frames.
    assert_eq!(f.call("Surface_Blt", &[dest, 0, dest, 0, 0, 0]), 0);
    assert_eq!(f.graphics.surfaces[&dest].pixels.as_slice(), copied);
    let held_self = f.graphics.take_packed_frame().unwrap();

    // Existing locked-destination behavior is unchanged: Unlock imports its guest bytes.
    let pointer = f.lock(dest);
    f.process.write(pointer, &copied).unwrap();
    assert_eq!(f.call("Surface_Blt", &[dest, 0, source, 0, 0, 0]), 0);
    assert_eq!(f.graphics.surfaces[&dest].pixels.as_slice(), old);
    assert_eq!(f.call("Surface_Unlock", &[dest, 0]), 0);
    assert_eq!(f.graphics.surfaces[&dest].pixels.as_slice(), copied);
    assert_eq!(held_self.pixels.as_slice(), copied);
}
