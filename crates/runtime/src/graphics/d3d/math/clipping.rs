//! Six-plane homogeneous clipping, including interpolated color and texture coordinates.
use super::ClipVertex;
fn distance(vertex: &ClipVertex, plane: usize) -> f64 {
    match plane {
        0 => vertex[0] + vertex[3],
        1 => vertex[3] - vertex[0],
        2 => vertex[1] + vertex[3],
        3 => vertex[3] - vertex[1],
        4 => vertex[2],
        _ => vertex[3] - vertex[2],
    }
}
pub(crate) fn clip(vertices: &[ClipVertex]) -> Vec<ClipVertex> {
    if vertices.len() > 32 {
        return Vec::new();
    }
    let mut polygon = vertices.to_vec();
    let mut result = Vec::new();
    for plane in 0..6 {
        let Some(&last) = polygon.last() else {
            break;
        };
        // Clipping an entirely inside polygon leaves every component unchanged.
        // Avoid rebuilding it for each of the six planes in the common case.
        if polygon.iter().all(|v| distance(v, plane) >= 0.0) {
            continue;
        }
        let mut a = last;
        let mut da = distance(&a, plane);
        result.clear();
        for &b in &polygon {
            let db = distance(&b, plane);
            if (da >= 0.0) != (db >= 0.0) {
                let t = da / (da - db);
                result.push(std::array::from_fn(|i| a[i] + (b[i] - a[i]) * t));
            }
            if db >= 0.0 {
                result.push(b);
            }
            a = b;
            da = db;
        }
        std::mem::swap(&mut polygon, &mut result);
    }
    polygon.retain(|vertex| vertex[3] > 0.0 && vertex[..4].iter().all(|n| n.is_finite()));
    polygon
}

#[cfg(test)]
#[path = "../../../../tests/unit/graphics/d3d/math/clipping.rs"]
mod tests;
