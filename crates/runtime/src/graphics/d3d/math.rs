//! Typed fixed-function math ported from the working runtime-rs/src/graphics/math.rs.
#[path = "math/clipping.rs"]
mod clipping;
#[path = "math/lighting.rs"]
mod lighting;
pub(super) use clipping::clip;
pub(super) use lighting::{light, Light, Material};
pub(super) type Mat4 = [f64; 16];
pub(super) type ClipVertex = [f64; 10];
pub(super) type RasterVertex = [f64; 7];

pub(super) fn unit(vector: [f64; 3]) -> [f64; 3] {
    let length = vector.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length == 0.0 {
        [0.0; 3]
    } else {
        vector.map(|x| x / length)
    }
}
pub(super) fn transform(vector: [f64; 4], matrix: &Mat4) -> [f64; 4] {
    std::array::from_fn(|column| {
        (0..4)
            .map(|row| vector[row] * matrix[row * 4 + column])
            .sum()
    })
}
pub(super) fn normal(vector: [f64; 3], matrix: &Mat4, normalize: bool) -> [f64; 3] {
    let [a, b, c, d, e, f, g, h, i] = [
        matrix[0], matrix[1], matrix[2], matrix[4], matrix[5], matrix[6], matrix[8], matrix[9],
        matrix[10],
    ];
    let cofactors = [
        e * i - f * h,
        f * g - d * i,
        d * h - e * g,
        c * h - b * i,
        a * i - c * g,
        b * g - a * h,
        b * f - c * e,
        c * d - a * f,
        a * e - b * d,
    ];
    let determinant = a * cofactors[0] + b * cofactors[1] + c * cofactors[2];
    let result = if determinant == 0.0 {
        [0.0; 3]
    } else {
        std::array::from_fn(|column| {
            (vector[0] * cofactors[column]
                + vector[1] * cofactors[3 + column]
                + vector[2] * cofactors[6 + column])
                / determinant
        })
    };
    if normalize {
        unit(result)
    } else {
        result
    }
}
pub(super) fn pack_color(color: [f64; 4]) -> u32 {
    let color = color.map(|x| (x.clamp(0.0, 255.0) + 0.5).floor() as u32);
    color[3] << 24 | color[0] << 16 | color[1] << 8 | color[2]
}
/// Viewport: x,y,width,height,minZ,maxZ,clipX,clipY,clipWidth,clipHeight.
/// Result: screen x,y,z,reciprocalW,packedARGB,u,v.
pub(super) fn project(vertex: ClipVertex, viewport: &[f64; 10]) -> RasterVertex {
    let reciprocal = 1.0 / vertex[3];
    [
        viewport[0] + (vertex[0] * reciprocal - viewport[6]) * viewport[2] / viewport[8],
        viewport[1] + (viewport[7] - vertex[1] * reciprocal) * viewport[3] / viewport[9],
        viewport[4] + vertex[2] * reciprocal * (viewport[5] - viewport[4]),
        reciprocal,
        pack_color([vertex[4], vertex[5], vertex[6], vertex[7]]) as f64,
        vertex[8],
        vertex[9],
    ]
}

#[cfg(test)]
#[path = "../../../tests/unit/graphics/d3d/math.rs"]
mod tests;
