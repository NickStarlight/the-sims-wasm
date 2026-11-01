use super::*;
fn options() -> Options<'static> {
    Options {
        clip: [0.0, 0.0, 256.0, 256.0],
        cull: 0,
        z_enable: 0,
        z_write: 0,
        z_func: 8,
        alpha_blend: 1,
        src_blend: 2,
        dst_blend: 1,
        alpha_test: 0,
        alpha_ref: 0,
        alpha_func: 8,
        texture: None,
        stage: [0; 7],
    }
}
#[test]
fn blend_alpha_obeys_source_and_destination_factors() {
    let mut p = options();
    let src = [20.0, 40.0, 60.0, 128.0];
    let dst = 0x40506070;
    assert_eq!(blend(&p, src, dst), src); // ONE/ZERO
    p.src_blend = 1;
    p.dst_blend = 2;
    assert_eq!(blend(&p, src, dst), unpack(dst)); // ZERO/ONE
    p.src_blend = 2;
    assert_eq!(blend(&p, src, dst)[3], 192.0); // additive
    p.src_blend = 5;
    p.dst_blend = 6;
    assert_eq!(
        blend(&p, src, dst)[3],
        128.0 * (128.0 / 255.0) + 64.0 * (1.0 - 128.0 / 255.0)
    );
    p.src_blend = 11;
    p.dst_blend = 1;
    assert_eq!(blend(&p, src, dst)[3], 128.0); // saturation leaves source alpha
}
#[test]
#[ignore = "manual scalar raster throughput benchmark"]
fn benchmark_triangle_raster() {
    let mut p = options();
    p.alpha_blend = 0;
    let mut pixels = vec![0; 256 * 256 * 4];
    let mut depth = vec![1.0; 256 * 256];
    let mut target = Target {
        pixels: &mut pixels,
        width: 256,
        height: 256,
        pitch: 1024,
        bpp: 32,
        depth: &mut depth,
        native_depth: None,
    };
    let vertices = [
        [0.0, 0.0, 0.5, 1.0, 0xffff0000u32 as f64, 0.0, 0.0],
        [256.0, 0.0, 0.5, 1.0, 0xff00ff00u32 as f64, 0.0, 0.0],
        [0.0, 256.0, 0.5, 1.0, 0xff0000ffu32 as f64, 0.0, 0.0],
    ];
    {
        let start = std::time::Instant::now();
        for _ in 0..200 {
            std::hint::black_box(triangle(&mut target, &p, vertices));
        }
        eprintln!("raster: {:?}", start.elapsed());
    }
}
