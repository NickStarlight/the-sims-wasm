use super::*;
use crate::graphics::tests::Fixture;
fn query(f: &mut Fixture, id: u32, guid: [u32; 4]) -> u32 {
    for (i, v) in guid.iter().enumerate() {
        f.word(600 + i as u32 * 4, *v);
    }
    assert_eq!(
        f.call(
            "D3D_QueryInterface",
            &[id, f.scratch + 600, f.scratch + 620]
        ),
        0
    );
    read_u32(&f.process, f.scratch + 620).unwrap()
}
fn scene(f: &mut Fixture, parent: u32, surface: u32) -> (u32, u32) {
    for (i, v) in interfaces::RGB.iter().enumerate() {
        f.word(600 + i as u32 * 4, *v);
    }
    assert_eq!(
        f.call(
            "D3D_CreateDevice",
            &[parent, f.scratch + 600, surface, f.scratch + 620, 0]
        ),
        0
    );
    let device = read_u32(&f.process, f.scratch + 620).unwrap();
    assert_eq!(
        f.call("D3D_CreateViewport", &[parent, f.scratch + 620, 0]),
        0
    );
    let viewport = read_u32(&f.process, f.scratch + 620).unwrap();
    assert_eq!(f.call("D3DDevice_AddViewport", &[device, viewport]), 0);
    assert_eq!(
        f.call("D3DDevice_SetCurrentViewport", &[device, viewport]),
        0
    );
    for (o, v) in [
        (700, 44),
        (704, 0),
        (708, 0),
        (712, 4),
        (716, 2),
        (720, 2f32.to_bits()),
        (724, 1f32.to_bits()),
        (728, 0),
        (732, 0),
        (736, 0),
        (740, 1f32.to_bits()),
    ] {
        f.word(o, v);
    }
    assert_eq!(
        f.call("D3DViewport_SetViewport", &[viewport, f.scratch + 700]),
        0
    );
    (device, viewport)
}
#[test]
fn discovery_and_full_com_draw_path_preserve_original_software_contract() {
    let mut f = Fixture::new();
    let dd = f.device;
    let parent = query(&mut f, dd, interfaces::PARENT3);
    let calls = f
        .graphics
        .d3d_enumerate(&mut f.process, "D3D_EnumDevices", &[parent, 0x401000, 7])
        .unwrap();
    assert_eq!(
        f.process.read_c_string(calls[0].1[2], 40).unwrap(),
        b"RGB Emulation"
    );
    assert_eq!(calls[0].1[5], 7);
    assert_eq!(
        read_u32(&f.process, calls[0].1[4] + 132).unwrap() & 1,
        1,
        "software enumeration advertises the renderer's perspective-correct textures"
    );
    let surface = f.surface(32, 0);
    let (device, viewport) = scene(&mut f, parent, surface);
    f.lock(surface);
    f.call("Surface_Unlock", &[surface, 0]);
    assert!(f.graphics.surfaces[&surface].guest_pixels_current);
    assert_eq!(
        f.call(
            "D3DViewport_Clear2",
            &[viewport, 0, 0, 3, 0xff000000, 1f32.to_bits(), 0]
        ),
        0
    );
    assert!(!f.graphics.surfaces[&surface].guest_pixels_current);
    f.lock(surface);
    f.call("Surface_Unlock", &[surface, 0]);
    assert_eq!(f.call("D3DDevice_BeginScene", &[device]), 0);
    assert_eq!(f.call("D3DDevice_SetRenderState", &[device, 22, 1]), 0);
    let points = [[0., 0.], [4., 0.], [0., 2.], [4., 0.], [4., 2.], [0., 2.]];
    for (i, xy) in points.iter().enumerate() {
        let offset = 800 + i as u32 * 20;
        for (j, value) in [xy[0], xy[1], 0.5, 1.].iter().enumerate() {
            f.word(offset + j as u32 * 4, (*value as f32).to_bits());
        }
        f.word(offset + 16, 0xffff0000);
    }
    assert_eq!(
        f.call(
            "D3DDevice_DrawPrimitive",
            &[device, 4, 0x44, f.scratch + 800, 6, 0]
        ),
        0
    );
    assert_eq!(f.call("D3DDevice_EndScene", &[device]), 0);
    let pixels = &f.graphics.surfaces[&surface].pixels;
    assert!(pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|b| u32::from_le_bytes(*b) == 0xffff0000));
    assert!(!f.graphics.surfaces[&surface].guest_pixels_current);
    let pointer = f.lock(surface);
    let mut guest_pixels = [0; 32];
    f.process.read(pointer, &mut guest_pixels).unwrap();
    assert!(guest_pixels
        .as_chunks::<4>()
        .0
        .iter()
        .all(|p| u32::from_le_bytes(*p) == 0xffff0000));
    f.call("Surface_Unlock", &[surface, 0]);
    assert_eq!(f.graphics.d3d.devices[&device].triangles, 2);
    let alias = query(
        &mut f,
        device,
        [0x93281501, 0x11d08cf8, 0xa000ab89, 0x294105c9],
    );
    assert_ne!(alias, device);
    assert_eq!(
        f.call("D3DDevice2_GetDirect3D", &[alias, f.scratch + 620]),
        NOT_IMPLEMENTED
    );
}

#[test]
fn indexed_xyz_normals_render_with_parallel_point_light_on_translated_geometry() {
    let mut f = Fixture::new();
    let dd = f.device;
    let parent = query(&mut f, dd, interfaces::PARENT3);
    let surface = f.surface(32, 0);
    let (device, viewport) = scene(&mut f, parent, surface);
    assert_eq!(f.call("D3D_CreateLight", &[parent, f.scratch + 620, 0]), 0);
    let source = read_u32(&f.process, f.scratch + 620).unwrap();
    // The mesh origin is z=-5; light z=-2 shines toward it along -Z. Range
    // and attenuation deliberately cannot illuminate this geometry as a point light.
    for (offset, value) in [
        (1200, 80),
        (1204, 4),
        (1212, 1f32.to_bits()),
        (1232, (-2f32).to_bits()),
        (1244, 1f32.to_bits()),
        (1276, 3),
    ] {
        f.word(offset, value);
    }
    assert_eq!(f.call("D3DLight_SetLight", &[source, f.scratch + 1200]), 0);
    assert_eq!(f.call("D3DViewport_AddLight", &[viewport, source]), 0);
    let mut world = state::identity();
    world[14] = -5.;
    for (i, value) in world.into_iter().enumerate() {
        f.word(1100 + i as u32 * 4, (value as f32).to_bits());
    }
    assert_eq!(
        f.call("D3DDevice_SetTransform", &[device, 1, f.scratch + 1100]),
        0
    );
    for (i, xy) in [[-1., 1.], [1., 1.], [-1., -1.], [1., -1.]]
        .into_iter()
        .enumerate()
    {
        for (j, value) in [xy[0], xy[1], 5.5, 0., 0., 1., 0., 0.]
            .into_iter()
            .enumerate()
        {
            f.word(
                800 + (i as u32 * 8 + j as u32) * 4,
                (value as f32).to_bits(),
            );
        }
    }
    f.process
        .write(f.scratch + 1000, &[0, 0, 1, 0, 2, 0, 1, 0, 3, 0, 2, 0])
        .unwrap();
    assert_eq!(f.call("D3DDevice_BeginScene", &[device]), 0);
    assert_eq!(f.call("D3DDevice_SetRenderState", &[device, 22, 1]), 0);
    let args = [device, 4, 0x112, f.scratch + 800, 4, f.scratch + 1000, 6, 4];
    assert_eq!(f.call("D3DDevice_DrawIndexedPrimitive", &args), 0);
    assert!(
        f.graphics.surfaces[&surface]
            .pixels
            .as_chunks::<4>()
            .0
            .iter()
            .all(|pixel| u32::from_le_bytes(*pixel) == 0xff00ff00),
        "parallel light must illuminate every vertex from the same geometry-relative direction"
    );
    f.word(1204, 5);
    assert_eq!(f.call("D3DLight_SetLight", &[source, f.scratch + 1200]), 0);
    assert_eq!(
        f.call("D3DDevice_DrawIndexedPrimitive", &args),
        NOT_IMPLEMENTED,
        "OpenGL spotlights remain unsupported"
    );
    assert_eq!(f.call("D3DDevice_EndScene", &[device]), 0);
}

#[test]
fn texture_load_invalidates_a_previously_synchronized_guest_buffer() {
    let mut f = Fixture::new();
    let source = f.surface(16, 0);
    let destination = f.surface(16, 0);
    // Fixture descriptors omit channel masks; texture loading needs RGB565 masks.
    for id in [source, destination] {
        f.graphics.surfaces.get_mut(&id).unwrap().pixel_format = None;
    }
    let pointer = f.lock(source);
    f.process.write(pointer, &[0, 248].repeat(8)).unwrap();
    f.call("Surface_Unlock", &[source, 0]);
    f.lock(destination);
    f.call("Surface_Unlock", &[destination, 0]);
    f.graphics.d3d.textures.insert(100, source);
    f.graphics.d3d.textures.insert(200, destination);
    assert_eq!(f.call("D3DTexture_Load", &[200, 100]), 0);
    assert!(!f.graphics.surfaces[&destination].guest_pixels_current);
    let pointer = f.lock(destination);
    let mut bytes = [0; 16];
    f.process.read(pointer, &mut bytes).unwrap();
    assert_eq!(bytes.as_slice(), &[0, 248].repeat(8));
}
