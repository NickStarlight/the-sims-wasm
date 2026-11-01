use super::*;
fn reference_clip(vertices: &[ClipVertex]) -> Vec<ClipVertex> {
    if vertices.len() > 32 {
        return Vec::new();
    }
    let mut polygon = vertices.to_vec();
    for plane in 0..6 {
        let Some(&last) = polygon.last() else {
            break;
        };
        let mut a = last;
        let mut da = distance(&a, plane);
        let mut result = Vec::new();
        for b in polygon {
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
        polygon = result;
    }
    polygon.retain(|vertex| vertex[3] > 0.0 && vertex[..4].iter().all(|n| n.is_finite()));
    polygon
}

#[test]
fn clipping_matches_reference_for_inside_outside_and_invalid_vertices() {
    let mut seed = 0x12345678u32;
    for count in [0, 1, 2, 3, 4, 16, 32, 33] {
        for case in 0..128 {
            let vertices: Vec<ClipVertex> = (0..count)
                .map(|_| {
                    std::array::from_fn(|component| {
                        seed ^= seed << 13;
                        seed ^= seed >> 17;
                        seed ^= seed << 5;
                        let v = (seed % 8192) as f64 / 1024.0 - 4.0;
                        if component == 3 && case < 64 {
                            5.0
                        } else {
                            v
                        }
                    })
                })
                .collect();
            let mut inputs = vec![vertices.clone()];
            for special in [0.0, -0.0, f64::INFINITY, f64::NEG_INFINITY, f64::NAN] {
                for component in 0..4 {
                    let mut input = vertices.clone();
                    if let Some(first) = input.first_mut() {
                        first[component] = special;
                    }
                    inputs.push(input);
                }
            }
            for input in inputs {
                let actual = clip(&input);
                let expected = reference_clip(&input);
                assert_eq!(actual.len(), expected.len());
                for (a, b) in actual.iter().zip(&expected) {
                    assert_eq!(a.map(f64::to_bits), b.map(f64::to_bits));
                }
            }
        }
    }
}
