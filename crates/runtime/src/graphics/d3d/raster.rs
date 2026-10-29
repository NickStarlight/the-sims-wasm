//! Original integer-center rasterizer, adapted from runtime-rs/src/graphics/raster.rs.
use super::{math, texture};
#[derive(Clone, Copy)]
struct Vertex {
    x: f64,
    y: f64,
    z: f64,
    rhw: f64,
    color: [f64; 4],
    u: f64,
    v: f64,
}
impl From<[f64; 7]> for Vertex {
    fn from(p: [f64; 7]) -> Self {
        Self {
            x: p[0],
            y: p[1],
            z: p[2],
            rhw: p[3],
            color: unpack(p[4] as u32),
            u: p[5],
            v: p[6],
        }
    }
}
pub(super) struct Target<'a> {
    pub pixels: &'a mut [u8],
    pub width: u32,
    pub height: u32,
    pub pitch: u32,
    pub bpp: u32,
    pub depth: &'a mut [f32],
    pub native_depth: Option<(&'a mut [u8], u32)>,
}
impl Target<'_> {
    fn pixel(&self, x: u32, y: u32) -> usize {
        (y * self.pitch + x * (self.bpp / 8)) as usize
    }
    fn get_depth(&self, x: u32, y: u32) -> f64 {
        if let Some((bytes, pitch)) = &self.native_depth {
            let i = (y * pitch + x * 2) as usize;
            u16::from_le_bytes(bytes[i..i + 2].try_into().unwrap()) as f64 / 65535.0
        } else {
            self.depth[(y * self.width + x) as usize] as f64
        }
    }
    fn set_depth(&mut self, x: u32, y: u32, z: f64) {
        self.depth[(y * self.width + x) as usize] = z as f32;
        if let Some((bytes, pitch)) = &mut self.native_depth {
            let i = (y * *pitch + x * 2) as usize;
            bytes[i..i + 2].copy_from_slice(&((z * 65535.0 + 0.5).floor() as u16).to_le_bytes());
        }
    }
    fn read(&self, x: u32, y: u32) -> u32 {
        let i = self.pixel(x, y);
        if self.bpp == 32 {
            u32::from_le_bytes(self.pixels[i..i + 4].try_into().unwrap())
        } else {
            let v = u16::from_le_bytes(self.pixels[i..i + 2].try_into().unwrap()) as u32;
            0xff000000
                | byte((v >> 11) as f64 * 255.0 / 31.0) << 16
                | byte(((v >> 5) & 63) as f64 * 255.0 / 63.0) << 8
                | byte((v & 31) as f64 * 255.0 / 31.0)
        }
    }
    fn write(&mut self, x: u32, y: u32, color: u32) {
        let i = self.pixel(x, y);
        if self.bpp == 32 {
            self.pixels[i..i + 4].copy_from_slice(&color.to_le_bytes());
        } else {
            self.pixels[i..i + 2].copy_from_slice(
                &((((color >> 19) & 31) << 11 | ((color >> 10) & 63) << 5 | ((color >> 3) & 31))
                    as u16)
                    .to_le_bytes(),
            );
        }
    }
    pub fn clear(&mut self, rect: [f64; 4], color: u32, z: f64, flags: u32) {
        let [l, t, r, b] = rect;
        for y in (t.ceil().max(0.0) as u32)..(b.ceil().min(self.height as f64).max(0.0) as u32) {
            for x in (l.ceil().max(0.0) as u32)..(r.ceil().min(self.width as f64).max(0.0) as u32) {
                if flags & 1 != 0 {
                    self.write(x, y, color);
                }
                if flags & 2 != 0 {
                    self.set_depth(x, y, z);
                }
            }
        }
    }
}
pub(super) struct Options<'a> {
    pub clip: [f64; 4],
    pub cull: u32,
    pub z_enable: u32,
    pub z_write: u32,
    pub z_func: u32,
    pub alpha_blend: u32,
    pub src_blend: u32,
    pub dst_blend: u32,
    pub alpha_test: u32,
    pub alpha_ref: u32,
    pub alpha_func: u32,
    pub texture: Option<texture::TextureSampler<'a>>,
    pub stage: [u32; 7],
}
fn edge(a: Vertex, b: Vertex, x: f64, y: f64) -> f64 {
    (b.x - a.x) * (y - a.y) - (b.y - a.y) * (x - a.x)
}
fn top_left(a: Vertex, b: Vertex) -> bool {
    b.y < a.y || (b.y == a.y && b.x > a.x)
}
fn shade(p: &Options, vertices: [Vertex; 3], weights: [f64; 3]) -> Option<[f64; 4]> {
    let mut color =
        std::array::from_fn(|i| (0..3).map(|j| vertices[j].color[i] * weights[j]).sum());
    if let Some(texture) = &p.texture {
        let rhw = (0..3).map(|i| vertices[i].rhw * weights[i]).sum::<f64>();
        if rhw == 0.0 {
            return None;
        }
        let u = (0..3)
            .map(|i| vertices[i].u * vertices[i].rhw * weights[i])
            .sum::<f64>()
            / rhw;
        let v = (0..3)
            .map(|i| vertices[i].v * vertices[i].rhw * weights[i])
            .sum::<f64>()
            / rhw;
        color = unpack(texture::combine(
            &p.stage,
            math::pack_color(color),
            texture.sample(u, v),
        ));
    }
    Some(color)
}
fn blend(p: &Options, mut src: [f64; 4], dst: u32) -> [f64; 4] {
    let dst = unpack(dst);
    let sf = p.src_blend;
    let df = p.dst_blend;
    let sa = src[3];
    let da = dst[3];
    for i in 0..3 {
        src[i] = src[i] * blend_factor(sf, src[i], dst[i], sa, da)
            + dst[i] * blend_factor(df, src[i], dst[i], sa, da);
    }
    src[3] = sa + da * (1.0 - sa / 255.0);
    src
}
pub(super) fn triangle(s: &mut Target, p: &Options, vertices: [[f64; 7]; 3]) -> u32 {
    let a = Vertex::from(vertices[0]);
    let mut b = Vertex::from(vertices[1]);
    let mut c = Vertex::from(vertices[2]);
    if ![a, b, c]
        .iter()
        .all(|v| [v.x, v.y, v.z, v.rhw].iter().all(|x| x.is_finite()))
    {
        return 0;
    }
    let mut area = edge(a, b, c.x, c.y);
    let cull = p.cull;
    if area == 0.0 || (cull == 2 && area > 0.0) || (cull == 3 && area < 0.0) {
        return 0;
    }
    if area < 0.0 {
        core::mem::swap(&mut b, &mut c);
        area = -area;
    }
    let left = (p.clip[0])
        .ceil()
        .max(a.x.min(b.x).min(c.x).ceil())
        .max(0.0) as i32;
    let top = (p.clip[1])
        .ceil()
        .max(a.y.min(b.y).min(c.y).ceil())
        .max(0.0) as i32;
    let right = (s.width as f64 - 1.0)
        .min(p.clip[2].ceil() - 1.0)
        .min(a.x.max(b.x).max(c.x).floor()) as i32;
    let bottom = (s.height as f64 - 1.0)
        .min(p.clip[3].ceil() - 1.0)
        .min(a.y.max(b.y).max(c.y).floor()) as i32;
    let edges = [top_left(b, c), top_left(c, a), top_left(a, b)];
    let mut pixels = 0;
    for y in top..=bottom {
        for x in left..=right {
            let e = [
                edge(b, c, x as f64, y as f64),
                edge(c, a, x as f64, y as f64),
                edge(a, b, x as f64, y as f64),
            ];
            if (0..3).any(|i| e[i] < 0.0 || (e[i] == 0.0 && !edges[i])) {
                continue;
            }
            let w = e.map(|v| v / area);
            let z = a.z * w[0] + b.z * w[1] + c.z * w[2];
            let x = x as u32;
            let y = y as u32;
            if !(0.0..=1.0).contains(&z) {
                continue;
            }
            let depth_z = if s.native_depth.is_some() {
                (z * 65535.0 + 0.5).floor() / 65535.0
            } else {
                z
            };
            if p.z_enable != 0 && !compare(p.z_func, depth_z, s.get_depth(x, y)) {
                continue;
            }
            let Some(mut color) = shade(p, [a, b, c], w) else {
                continue;
            };
            if p.alpha_test != 0
                && !compare(p.alpha_func, (color[3] + 0.5).floor(), p.alpha_ref as f64)
            {
                continue;
            }
            if p.alpha_blend != 0 {
                color = blend(p, color, s.read(x, y));
            }
            s.write(x, y, math::pack_color(color));
            if p.z_enable != 0 && p.z_write != 0 {
                s.set_depth(x, y, z);
            }
            pixels += 1;
        }
    }
    pixels
}

fn byte(v: f64) -> u32 {
    (v.clamp(0.0, 255.0) + 0.5).floor() as u32
}
fn unpack(v: u32) -> [f64; 4] {
    [
        (v >> 16 & 255) as f64,
        (v >> 8 & 255) as f64,
        (v & 255) as f64,
        (v >> 24) as f64,
    ]
}
fn compare(function: u32, a: f64, b: f64) -> bool {
    match function {
        2 => a < b,
        3 => a == b,
        4 => a <= b,
        5 => a > b,
        6 => a != b,
        7 => a >= b,
        8 => true,
        _ => false,
    }
}
fn blend_factor(function: u32, src: f64, dst: f64, sa: f64, da: f64) -> f64 {
    match function {
        1 => 0.0,
        2 => 1.0,
        3 => src / 255.0,
        4 => 1.0 - src / 255.0,
        5 => sa / 255.0,
        6 => 1.0 - sa / 255.0,
        7 => da / 255.0,
        8 => 1.0 - da / 255.0,
        9 => dst / 255.0,
        10 => 1.0 - dst / 255.0,
        11 => sa.min(255.0 - da) / 255.0,
        _ => 1.0,
    }
}
