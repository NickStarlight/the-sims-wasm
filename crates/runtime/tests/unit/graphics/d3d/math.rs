use super::*;
fn identity() -> Mat4 {
    [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ]
}
fn near<const N: usize>(actual: [f64; N], expected: [f64; N]) {
    for (a, b) in actual.into_iter().zip(expected) {
        assert!((a - b).abs() < 1e-10, "{a} != {b}");
    }
}
#[test]
fn row_vector_transforms_inverse_transpose_normals_and_projection_match_source() {
    let mut matrix = identity();
    matrix[0] = 2.0;
    matrix[5] = 3.0;
    matrix[10] = 4.0;
    matrix[12] = 10.0;
    matrix[13] = 20.0;
    matrix[14] = 30.0;
    near(
        transform([1.0, 2.0, 3.0, 1.0], &matrix),
        [12.0, 26.0, 42.0, 1.0],
    );
    near(
        normal([1.0, 1.0, 1.0], &matrix, false),
        [0.5, 1.0 / 3.0, 0.25],
    );
    near(normal([0.0, 0.0, 2.0], &matrix, true), [0.0, 0.0, 1.0]);
    assert_eq!(normal([1.0; 3], &[0.0; 16], false), [0.0; 3]);
    assert_eq!(unit([0.0; 3]), [0.0; 3]);
    let projected = project(
        [0.0, 0.0, 0.5, 1.0, 255.0, 128.0, 0.0, 64.0, 0.25, 0.75],
        &[10.0, 20.0, 800.0, 600.0, 0.0, 1.0, -1.0, 1.0, 2.0, 2.0],
    );
    near(
        projected,
        [410.0, 320.0, 0.5, 1.0, 0x40ff8000u32 as f64, 0.25, 0.75],
    );
    assert_eq!(pack_color([-1.0, 127.5, 300.0, 255.0]), 0xff0080ff);
}
#[test]
fn near_plane_clipping_interpolates_colors_uv_and_rejects_invalid_w() {
    let polygon = clip(&[
        [0.0, 0.0, -1.0, 1.0, 100.0, 0.0, 0.0, 255.0, 0.0, 0.0],
        [0.5, 0.0, 1.0, 1.0, 200.0, 0.0, 0.0, 255.0, 1.0, 0.0],
        [0.0, 0.5, 1.0, 1.0, 0.0, 0.0, 0.0, 255.0, 0.0, 1.0],
    ]);
    assert_eq!(polygon.len(), 4);
    assert!(polygon.iter().all(|v| v[2] >= 0.0 && v[2] <= v[3]));
    assert!(polygon
        .iter()
        .any(|v| v[2] == 0.0 && v[4] == 150.0 && v[8] == 0.5));
    assert!(polygon
        .iter()
        .any(|v| v[2] == 0.0 && v[4] == 50.0 && v[9] == 0.5));
    assert!(clip(&[[0.0; 10]; 33]).is_empty());
    assert!(clip(&[[0.0; 10]; 3]).is_empty());
    let outside = [[4.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]; 3];
    assert!(clip(&outside).is_empty());
}
fn source_light() -> Light {
    Light {
        kind: 3,
        color: [1.0; 3],
        position: [0.0, 0.0, 2.0],
        direction: [0.0, 0.0, -1.0],
        range: 10.0,
        falloff: 1.0,
        attenuation: [1.0, 1.0, 0.0],
        theta: 0.0,
        phi: std::f64::consts::FRAC_PI_2,
        enabled: true,
    }
}
#[test]
fn directional_point_spot_ambient_and_material_alpha_keep_old_lighting_rules() {
    near(
        light(
            [0.0; 3],
            [0.0; 3],
            [0.0, 0.0, 1.0],
            None,
            0,
            &[source_light()],
        ),
        [255.0; 4],
    );
    let point = Light {
        kind: 1,
        ..source_light()
    };
    near(
        light([0.0; 3], [0.0; 3], [0.0, 0.0, 1.0], None, 0, &[point]),
        [85.0, 85.0, 85.0, 255.0],
    );
    let distant = Light {
        range: 1.0,
        ..point
    };
    near(
        light([0.0; 3], [0.0; 3], [0.0, 0.0, 1.0], None, 0, &[distant]),
        [0.0, 0.0, 0.0, 255.0],
    );
    let spot = Light { kind: 2, ..point };
    near(
        light([0.0; 3], [0.0; 3], [0.0, 0.0, 1.0], None, 0, &[spot]),
        [85.0, 85.0, 85.0, 255.0],
    );
    let sideways = Light {
        direction: [1.0, 0.0, 0.0],
        ..spot
    };
    near(
        light([0.0; 3], [0.0; 3], [0.0, 0.0, 1.0], None, 0, &[sideways]),
        [0.0, 0.0, 0.0, 255.0],
    );
    let material = Material {
        diffuse: [1.0, 1.0, 1.0, 0.5],
        ambient: [1.0; 3],
        emissive: [0.1, 0.0, 0.0],
    };
    near(
        light(
            [0.0; 3],
            [0.0; 3],
            [0.0; 3],
            Some(&material),
            0x00204060,
            &[Light {
                enabled: false,
                ..point
            }],
        ),
        [57.5, 64.0, 96.0, 127.5],
    );
}

#[test]
fn parallel_point_uses_geometry_origin_without_vertex_distance_attenuation() {
    let source = Light {
        kind: 4,
        position: [0., 0., 10.],
        direction: [0., 0., 1.], // Deliberately opposite to the emitted light.
        range: 0.1,
        attenuation: [0.; 3],
        ..source_light()
    };
    for vertex in [[-20., 3., 15.], [25., 0., -100.]] {
        near(
            light(vertex, [0.; 3], [0., 0., 1.], None, 0, &[source]),
            [255.; 4],
        );
        near(
            light(vertex, [0., 0., 20.], [0., 0., 1.], None, 0, &[source]),
            [0., 0., 0., 255.],
        );
        near(
            light(vertex, [0., 0., 20.], [0., 0., -1.], None, 0, &[source]),
            [255.; 4],
        );
    }
    near(
        light([0.; 3], source.position, [0., 0., 1.], None, 0, &[source]),
        [0., 0., 0., 255.],
    );
}

#[test]
fn intrinsic_color_rounding_and_normalization_match_library_math() {
    for i in 0..65536 {
        let x = i as f64 / 257.0 - 1.0;
        let channels = [x, x + 0.5, -x, 255.5 - x];
        let expected = channels.map(|v| libm::floor(v.clamp(0.0, 255.0) + 0.5) as u32);
        assert_eq!(
            pack_color(channels),
            expected[3] << 24 | expected[0] << 16 | expected[1] << 8 | expected[2]
        );
        let vector = [x, x + 0.5, 255.5 - x];
        let length = libm::sqrt(vector.iter().map(|v| v * v).sum());
        assert_eq!(
            unit(vector).map(f64::to_bits),
            vector.map(|v| (v / length).to_bits())
        );
    }
}
