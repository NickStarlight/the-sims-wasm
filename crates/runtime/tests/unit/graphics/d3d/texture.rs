use super::*;
fn sampler(pixels: &[u8]) -> TextureSampler<'_> {
    TextureSampler {
        pixels,
        width: 2,
        height: 2,
        pitch: 8,
        bpp: 32,
        masks: [0xff0000, 0xff00, 0xff, 0xff000000],
        palette: None,
        color_key: None,
        address_u: 1,
        address_v: 1,
        linear: false,
        border: 0x12345678,
    }
}
fn pixels() -> Vec<u8> {
    [0xffff0000u32, 0xff00ff00, 0xff0000ff, 0xffffffff]
        .iter()
        .flat_map(|p| p.to_le_bytes())
        .collect()
}
#[test]
fn nearest_address_modes_bilinear_and_color_keys_match_source() {
    let pixels = pixels();
    let mut texture = sampler(&pixels);
    assert_eq!(texture.sample(0.1, 0.1), 0xffff0000);
    assert_eq!(texture.sample(1.1, 0.1), 0xffff0000);
    assert_eq!(texture.sample(-0.1, 0.1), 0xff00ff00);
    texture.address_u = 2;
    assert_eq!(texture.sample(1.1, 0.1), 0xff00ff00);
    assert_eq!(texture.sample(-0.1, 0.1), 0xffff0000);
    texture.address_u = 3;
    assert_eq!(texture.sample(1.1, 0.1), 0xff00ff00);
    texture.address_u = 4;
    assert_eq!(texture.sample(-0.1, 0.1), 0x12345678);
    texture.address_u = 3;
    texture.address_v = 3;
    texture.linear = true;
    assert_eq!(texture.sample(0.5, 0.5), 0xff808080);
    texture.linear = false;
    texture.color_key = Some((0xffff0000, 0xffff0000));
    assert_eq!(texture.sample(0.1, 0.1), 0);
    assert_eq!(texture.sample(f64::NAN, 0.0), 0);
}
#[test]
fn palette_masks_pitch_and_short_storage_use_checked_borrowed_data() {
    let palette = [[1, 2, 3, 0], [4, 5, 6, 0]];
    let mut texture = sampler(&[0, 1, 99, 99, 1, 0, 99, 99]);
    texture.width = 2;
    texture.height = 2;
    texture.bpp = 8;
    texture.pitch = 4;
    texture.palette = Some(&palette);
    assert_eq!(texture.sample(0.1, 0.7), 0xff040506);
    assert_eq!(texture.sample(0.7, 0.7), 0xff010203);
    texture.palette = None;
    assert_eq!(texture.sample(0.1, 0.1), 0);
    let storage = [0, 0xf8, 0xe0, 7];
    let mut texture = sampler(&storage);
    texture.height = 1;
    texture.bpp = 16;
    texture.pitch = 4;
    texture.masks = [0xf800, 0x7e0, 0x1f, 0];
    assert_eq!(texture.sample(0.1, 0.1), 0xffff0000);
    assert_eq!(texture.sample(0.7, 0.1), 0xff00ff00);
    texture.pixels = &storage[..1];
    assert_eq!(texture.sample(0.1, 0.1), 0);
    texture.width = 0;
    assert_eq!(texture.sample(0.1, 0.1), 0);
}
#[test]
fn stage_color_ops_argument_modifiers_and_alpha_disable_match_source() {
    let diffuse = 0x80402010;
    let texture = 0x4080ff20;
    assert_eq!(
        combine(&[2, 2, 0, 1, 0, 0, 0], diffuse, texture),
        0x8080ff20
    );
    assert_eq!(
        combine(&[3, 2, 0, 2, 2, 0, 0], diffuse, texture),
        0x40402010
    );
    assert_eq!(
        combine(&[4, 2, 0, 1, 0, 0, 0], diffuse, texture),
        0x80202002
    );
    assert_eq!(
        combine(&[2, 2 | 16, 0, 1, 0, 0, 0], diffuse, texture),
        0x807f00df
    );
    assert_eq!(
        combine(&[2, 2 | 32, 0, 1, 0, 0, 0], diffuse, texture),
        0x80404040
    );
    assert_eq!(
        combine(&[2, 3, 0, 1, 0, 0, 0x00112233], diffuse, texture),
        0x80112233
    );
    assert_eq!(
        combine(&[7, 2, 0, 1, 0, 0, 0], diffuse, texture),
        0x80c0ff30
    );
}
