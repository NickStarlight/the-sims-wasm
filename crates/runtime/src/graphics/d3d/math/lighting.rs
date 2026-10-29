//! World-space legacy diffuse lighting with typed inputs.
use super::unit;
#[derive(Clone, Copy, Debug)]
pub(crate) struct Material {
    pub diffuse: [f64; 4],
    pub ambient: [f64; 3],
    pub emissive: [f64; 3],
}
impl Default for Material {
    fn default() -> Self {
        Self {
            diffuse: [1.0; 4],
            ambient: [1.0; 3],
            emissive: [0.0; 3],
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub(crate) struct Light {
    pub kind: u32,
    pub color: [f64; 3],
    pub position: [f64; 3],
    pub direction: [f64; 3],
    pub range: f64,
    pub falloff: f64,
    pub attenuation: [f64; 3],
    pub theta: f64,
    pub phi: f64,
    pub enabled: bool,
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| a[i] * b[i]).sum()
}
pub(crate) fn light(
    position: [f64; 3],
    geometry_origin: [f64; 3],
    normal: [f64; 3],
    material: Option<&Material>,
    ambient: u32,
    lights: &[Light],
) -> [f64; 4] {
    let default = Material::default();
    let material = material.unwrap_or(&default);
    let mut color = std::array::from_fn::<_, 3, _>(|i| {
        ((ambient >> (16 - i * 8)) & 255) as f64 / 255.0 * material.ambient[i]
            + material.emissive[i]
    });
    for light in lights {
        if !light.enabled {
            continue;
        }
        let mut attenuation = 1.0;
        let direction = if light.kind == 3 {
            unit(light.direction.map(|x| -x))
        } else if light.kind == 4 {
            // D3DLIGHT_PARALLELPOINT uses one direction for the whole geometry,
            // from its origin toward the light. Like a directional light, it
            // ignores range, attenuation, and the supplied direction field.
            // DirectX 5 SDK: html/dire8c89.content.htm (D3DLIGHTTYPE).
            unit(std::array::from_fn(|i| {
                light.position[i] - geometry_origin[i]
            }))
        } else {
            let delta = std::array::from_fn(|i| light.position[i] - position[i]);
            let distance = dot(delta, delta).sqrt();
            if distance > light.range {
                continue;
            }
            let direction = unit(delta);
            let denominator = light.attenuation[0]
                + light.attenuation[1] * distance
                + light.attenuation[2] * distance * distance;
            attenuation = if denominator > 0.0 {
                1.0 / denominator
            } else {
                0.0
            };
            if light.kind == 2 {
                let rho = -dot(unit(light.direction), direction);
                let inner = libm::cos(light.theta / 2.0);
                let outer = libm::cos(light.phi / 2.0);
                if rho <= outer {
                    continue;
                }
                if rho < inner {
                    attenuation *= libm::pow((rho - outer) / (inner - outer), light.falloff);
                }
            }
            direction
        };
        let intensity = dot(normal, direction).max(0.0) * attenuation;
        for (i, value) in color.iter_mut().enumerate() {
            *value += material.diffuse[i] * light.color[i] * intensity;
        }
    }
    [
        color[0] * 255.0,
        color[1] * 255.0,
        color[2] * 255.0,
        material.diffuse[3] * 255.0,
    ]
    .map(|x| x.clamp(0.0, 255.0))
}
