use super::*;

#[test]
fn packed_fills_preserve_partial_rows_padding_and_completed_snapshots() {
    let mut f = super::super::tests::Fixture::new();
    for bits in [8, 16, 24, 32] {
        let id = f.surface(bits, 0);
        let mut original = f.graphics.surfaces[&id].clone();
        let bytes = original.format.bytes() as usize;
        original.width = 11;
        original.height = 7;
        for padding in [0, 1, 7] {
            original.pitch = original.width * bytes as u32 + padding;
            original.pixels = Rc::new(
                (0..original.pitch * original.height)
                    .map(|i| (i * 73 + 19) as u8)
                    .collect(),
            );
            for rect in [
                Rect {
                    left: 0,
                    top: 0,
                    right: 11,
                    bottom: 7,
                },
                Rect {
                    left: 1,
                    top: 2,
                    right: 8,
                    bottom: 6,
                },
                Rect {
                    left: 3,
                    top: 1,
                    right: 4,
                    bottom: 2,
                },
            ] {
                for color in [0, 0x12345678, u32::MAX] {
                    let mut expected = (*original.pixels).clone();
                    for y in rect.top..rect.bottom {
                        for x in rect.left..rect.right {
                            let offset = y as usize * original.pitch as usize + x as usize * bytes;
                            expected[offset..offset + bytes]
                                .copy_from_slice(&color.to_le_bytes()[..bytes]);
                        }
                    }
                    let mut actual = original.clone();
                    copy_pixels(
                        &mut actual,
                        None,
                        (rect, rect),
                        (rect.width(), rect.height()),
                        Some(color),
                        (Some(color), Some(0)),
                        None,
                    );
                    assert_eq!(
                        *actual.pixels, expected,
                        "bits={bits}, padding={padding}, color={color:#x}"
                    );
                    assert_eq!(
                        *original.pixels,
                        (0..original.pitch * original.height)
                            .map(|i| (i * 73 + 19) as u8)
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn optimized_blits_match_scalar_pixels_with_padding_keys_and_overlap() {
    let mut f = super::super::tests::Fixture::new();
    for bits in [8, 16, 24, 32] {
        let id = f.surface(bits, 0);
        let mut original = f.graphics.surfaces[&id].clone();
        let bytes = original.format.bytes() as usize;
        original.pitch += 4;
        original.pixels = Rc::new(
            (0..original.pitch * original.height)
                .map(|i| (i * 73 + 19) as u8)
                .collect(),
        );
        let sr = Rect {
            left: 0,
            top: 0,
            right: 3,
            bottom: 2,
        };
        let dr = Rect {
            left: 1,
            top: 0,
            right: 4,
            bottom: 2,
        };
        let source_value = format::read_pixel(&original, 1, 0);
        let dest_value = format::read_pixel(&original, 2, 0);
        for source_key in [None, Some(source_value), Some(u32::MAX)] {
            for destination_key in [None, Some(dest_value), Some(u32::MAX)] {
                for fill in [None, Some(0x12345678)] {
                    let mut expected = (*original.pixels).clone();
                    for y in 0..2 {
                        for x in 0..3 {
                            let old = format::read_pixel(&original, x + 1, y);
                            let value = format::read_pixel(&original, x, y);
                            if fill.is_none()
                                && (source_key == Some(value)
                                    || destination_key.is_some_and(|key| old != key))
                            {
                                continue;
                            }
                            let offset =
                                y as usize * original.pitch as usize + (x as usize + 1) * bytes;
                            expected[offset..offset + bytes]
                                .copy_from_slice(&fill.unwrap_or(value).to_le_bytes()[..bytes]);
                        }
                    }
                    let mut actual = original.clone();
                    copy_pixels(
                        &mut actual,
                        Some(&original),
                        (dr, sr),
                        (3, 2),
                        fill,
                        (source_key, destination_key),
                        None,
                    );
                    assert_eq!(*actual.pixels, expected, "bits={bits}, source={source_key:?}, destination={destination_key:?}, fill={fill:?}");
                    // Copy-on-write must not mutate the source snapshot.
                    assert_eq!(format::read_pixel(&original, 1, 0), source_value);
                }
            }
        }
    }
}
