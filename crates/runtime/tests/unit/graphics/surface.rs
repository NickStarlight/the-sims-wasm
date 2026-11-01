use super::*;
use crate::graphics::tests::Fixture;

fn write_image(f: &mut Fixture, surface: u32, bytes: &[u8]) -> u32 {
    let pointer = f.lock(surface);
    f.process.write(pointer, bytes).unwrap();
    assert_eq!(f.call("Surface_Unlock", &[surface, 0]), 0);
    pointer
}

#[test]
fn writable_unlock_reuses_retired_pixels_without_changing_completed_images() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0x200);
    let pointer = write_image(&mut f, id, &[1; 16]);
    let first_pixels = f.graphics.surfaces[&id].pixels.as_ptr();
    write_image(&mut f, id, &[2; 16]);
    let retired = f.graphics.recycled_pixels.as_ref().unwrap();
    assert_eq!(retired.as_ptr(), first_pixels);
    let second_pixels = f.graphics.surfaces[&id].pixels.as_ptr();
    write_image(&mut f, id, &[3; 16]);
    assert_eq!(f.graphics.surfaces[&id].pixels.as_ptr(), first_pixels);
    assert_eq!(
        f.graphics.recycled_pixels.as_ref().unwrap().as_ptr(),
        second_pixels
    );
    let held = f.graphics.take_packed_frame().unwrap();
    write_image(&mut f, id, &[4; 16]);
    write_image(&mut f, id, &[5; 16]);
    assert_eq!(held.pixels.as_slice(), &[3; 16]);
    assert_eq!(f.graphics.surfaces[&id].pixels.as_slice(), &[5; 16]);
    assert_eq!(f.lock(id), pointer);
    assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
    assert_eq!(held.pixels.as_slice(), &[3; 16]);
}

#[test]
fn packed_and_rgba_exports_stay_owned_across_subsequent_unlocks() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0x200);
    write_image(&mut f, id, &[0, 248].repeat(8));
    let first = f.graphics.take_packed_frame().unwrap();
    write_image(&mut f, id, &[224, 7].repeat(8));
    let second = f.graphics.take_packed_frame().unwrap();
    f.graphics.present(id).unwrap();
    let rgba = f.graphics.take_frame().unwrap();
    assert_eq!(rgba.pixels, [0, 255, 0, 255].repeat(8));
    write_image(&mut f, id, &[31, 0].repeat(8));
    let blue = f.graphics.take_frame().unwrap();
    assert_eq!(blue.pixels, [0, 0, 255, 255].repeat(8));
    assert_eq!(rgba.pixels, [0, 255, 0, 255].repeat(8));
    assert_eq!(first.pixels.as_slice(), [0, 248].repeat(8));
    assert_eq!(second.pixels.as_slice(), [224, 7].repeat(8));
}

#[test]
fn failed_unlock_keeps_the_lock_snapshot_and_recycled_storage_until_retry() {
    let mut f = Fixture::new();
    let id = f.surface(16, 0x200);
    let pointer = write_image(&mut f, id, &[1; 16]);
    write_image(&mut f, id, &[2; 16]);
    let recycled = f.graphics.recycled_pixels.as_ref().unwrap().as_ptr();
    assert_eq!(f.lock(id), pointer);
    // An unavailable backing range must be checked even when a reusable buffer
    // exists. Restore the range afterward to exercise the same lock's retry.
    f.graphics.surfaces.get_mut(&id).unwrap().guest_pixels = 0x500;
    assert!(matches!(
        f.graphics
            .call(&mut f.process, &f.modules, "Surface_Unlock", &[id, 0]),
        Err(Error::Unmapped)
    ));
    assert_eq!(f.graphics.surfaces[&id].lock, Some(false));
    assert!(!f.graphics.surfaces[&id].guest_pixels_current);
    assert_eq!(f.graphics.surfaces[&id].pixels.as_slice(), &[2; 16]);
    assert_eq!(
        f.graphics.frame.as_ref().unwrap().surface.pixels.as_slice(),
        &[2; 16]
    );
    assert_eq!(
        f.graphics.recycled_pixels.as_ref().unwrap().as_ptr(),
        recycled
    );
    f.graphics.surfaces.get_mut(&id).unwrap().guest_pixels = pointer;
    f.process.write(pointer, &[3; 16]).unwrap();
    assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
    assert_eq!(f.graphics.surfaces[&id].pixels.as_ptr(), recycled);
    assert_eq!(f.graphics.surfaces[&id].pixels.as_slice(), &[3; 16]);
}

#[test]
fn full_unlock_and_frame_ownership_match_uncached_oracle_across_formats() {
    for bits in [8, 16, 24, 32] {
        let mut actual = Fixture::new();
        let mut oracle = Fixture::new();
        let actual_ids = [actual.surface(bits, 0x200), actual.surface(bits, 0)];
        let oracle_ids = [oracle.surface(bits, 0x200), oracle.surface(bits, 0)];
        let mut held_actual = Vec::new();
        let mut held_oracle = Vec::new();
        for step in 0..96u32 {
            for (f, ids, held, cached) in [
                (&mut actual, actual_ids, &mut held_actual, true),
                (&mut oracle, oracle_ids, &mut held_oracle, false),
            ] {
                if !cached {
                    f.graphics.recycled_pixels = None;
                }
                let id = ids[(step % 3 == 0) as usize];
                // Legacy locks validate the subrectangle but still expose and
                // import the complete image, regardless of the old flag hints.
                for (offset, value) in [(512, 1), (516, 0), (520, 2), (524, 1), (256, 108)] {
                    f.word(offset, value);
                }
                assert_eq!(
                    f.call(
                        "Surface_Lock",
                        &[id, f.scratch + 512, f.scratch + 256, step & 0x30, 0]
                    ),
                    0
                );
                let pointer = read_u32(&f.process, f.scratch + 292).unwrap();
                let length = f.graphics.surfaces[&id].pixels.len();
                let bytes: Vec<_> = (0..length)
                    .map(|i| (i as u32 * 71 + step * 13) as u8)
                    .collect();
                f.process.write(pointer, &bytes).unwrap();
                assert_eq!(f.call("Surface_Unlock", &[id, 0]), 0);
                if id == ids[1] {
                    assert_eq!(f.call("Surface_Blt", &[ids[0], 0, ids[1], 0, 0, 0]), 0);
                }
                if step % 5 == 0 {
                    held.push(f.graphics.take_packed_frame().unwrap());
                    if held.len() > 4 {
                        held.remove(0);
                    }
                }
            }
            for (left, right) in actual_ids.iter().zip(oracle_ids) {
                let a = &actual.graphics.surfaces[left];
                let b = &oracle.graphics.surfaces[&right];
                assert_eq!(a.pixels, b.pixels, "{bits}-bit step {step}");
                assert_eq!(
                    (a.lock, a.guest_pixels_current),
                    (b.lock, b.guest_pixels_current)
                );
            }
            assert_eq!(held_actual.len(), held_oracle.len());
            for (left, right) in held_actual.iter().zip(&held_oracle) {
                assert_eq!(
                    (left.width, left.height, left.pitch, left.format),
                    (right.width, right.height, right.pitch, right.format)
                );
                assert_eq!(
                    left.pixels, right.pixels,
                    "retained {bits}-bit frame at step {step}"
                );
            }
        }
    }
}
