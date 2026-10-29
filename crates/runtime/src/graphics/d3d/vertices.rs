//! FVF decoding and world/view/projection conversion from the original renderer.
use super::*;
pub(super) fn stride(fvf: u32) -> usize {
    (if fvf & 14 == 4 { 16 } else { 12 })
        + if fvf & 16 != 0 { 12 } else { 0 }
        + if fvf & 64 != 0 { 4 } else { 0 }
        + if fvf & 128 != 0 { 4 } else { 0 }
        + if fvf & 256 != 0 { 8 } else { 0 }
}
pub(super) fn decode(
    bytes: &[u8],
    fvf: u32,
    device: &device::Device,
    viewport: &Viewport,
    material: Option<&math::Material>,
    lights: &[math::Light],
) -> [f64; 10] {
    let f = |offset| resources::float(bytes, offset);
    let transformed = fvf & 14 == 4;
    let mut offset = if transformed { 16 } else { 12 };
    let normal = if fvf & 16 != 0 {
        let value = std::array::from_fn(|i| f(offset + i * 4));
        offset += 12;
        value
    } else {
        [0.; 3]
    };
    let color = if fvf & 64 != 0 {
        let value = resources::word(bytes, offset);
        offset += 4;
        value
    } else {
        u32::MAX
    };
    if fvf & 128 != 0 {
        offset += 4;
    }
    let uv = if fvf & 256 != 0 {
        [f(offset), f(offset + 4)]
    } else {
        [0.; 2]
    };
    let position = [f(0), f(4), f(8), 1.];
    if transformed {
        return [
            position[0],
            position[1],
            position[2],
            f(12),
            color as f64,
            uv[0],
            uv[1],
            0.,
            0.,
            0.,
        ];
    }
    let world = math::transform(position, &device.matrices[0]);
    let mut clip = math::transform(
        math::transform(world, &device.matrices[1]),
        &device.matrices[2],
    );
    for i in 0..3 {
        clip[i] = clip[i] * viewport.scale[i] + viewport.offset[i] * clip[3];
    }
    let rgba = if fvf & 16 != 0 {
        math::light(
            [world[0], world[1], world[2]],
            std::array::from_fn(|i| device.matrices[0][12 + i]),
            math::normal(
                normal,
                &device.matrices[0],
                device.render.get(139).unwrap_or(0) != 0,
            ),
            material,
            device.light_states.get(2).unwrap_or(0),
            lights,
        )
    } else {
        [
            (color >> 16 & 255) as f64,
            (color >> 8 & 255) as f64,
            (color & 255) as f64,
            (color >> 24) as f64,
        ]
    };
    [
        clip[0], clip[1], clip[2], clip[3], rgba[0], rgba[1], rgba[2], rgba[3], uv[0], uv[1],
    ]
}
pub(super) fn material(m: &Material) -> math::Material {
    let v = |o| resources::float(&m.bytes, o);
    math::Material {
        diffuse: std::array::from_fn(|i| v(4 + i * 4)),
        ambient: std::array::from_fn(|i| v(20 + i * 4)),
        emissive: std::array::from_fn(|i| v(52 + i * 4)),
    }
}
pub(super) fn light(l: &Light) -> math::Light {
    let v = |o| resources::float(&l.bytes, o);
    math::Light {
        kind: resources::word(&l.bytes, 4),
        color: std::array::from_fn(|i| v(8 + i * 4)),
        position: std::array::from_fn(|i| v(24 + i * 4)),
        direction: std::array::from_fn(|i| v(36 + i * 4)),
        range: v(48),
        falloff: v(52),
        attenuation: std::array::from_fn(|i| v(56 + i * 4)),
        theta: v(68),
        phi: v(72),
        enabled: resources::word(&l.bytes, 76) & 1 != 0,
    }
}
