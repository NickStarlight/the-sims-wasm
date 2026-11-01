//! Viewport clear contracts with the original rectangle/depth clipping.
use super::*;
impl Graphics {
    pub(super) fn d3d_clear<M: GuestMemory>(
        &mut self,
        p: &Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let view = &self.d3d.viewports[&a[0]];
        let Some(id) = self.d3d.device_id(view.device) else {
            return Ok(INVALID);
        };
        if a[3] & !3 != 0 {
            return Ok(NOT_IMPLEMENTED);
        }
        if a[1] > MAX_BYTES as u32 / 16 || (a[1] != 0 && a[2] == 0) {
            return Ok(INVALID);
        }
        let extended = name.ends_with("Clear2");
        let z = if extended {
            f32::from_bits(a[5]) as f64
        } else {
            1.
        };
        if !z.is_finite() || !(0.0..=1.0).contains(&z) {
            return Ok(INVALID);
        }
        let color = if extended {
            a[4]
        } else {
            self.d3d
                .materials
                .values()
                .find(|m| m.handle == view.background)
                .map(resources::material_color)
                .unwrap_or(0xff000000)
        };
        let clip = [
            view.rect[0] as f64,
            view.rect[1] as f64,
            view.rect[0] as f64 + view.rect[2] as f64,
            view.rect[1] as f64 + view.rect[3] as f64,
        ];
        let rects = if a[1] == 0 {
            vec![clip]
        } else {
            let mut bytes = vec![0; a[1] as usize * 16];
            p.read(a[2], &mut bytes)?;
            bytes
                .as_chunks::<16>()
                .0
                .iter()
                .map(|b| {
                    let r: [f64; 4] = std::array::from_fn(|i| {
                        i32::from_le_bytes(b[i * 4..i * 4 + 4].try_into().unwrap()) as f64
                    });
                    [
                        clip[0].max(r[0]),
                        clip[1].max(r[1]),
                        clip[2].min(r[2]),
                        clip[3].min(r[3]),
                    ]
                })
                .collect()
        };
        self.with_d3d_target(id, |target| {
            for rect in rects {
                target.clear(rect, color, z, a[3]);
            }
        })?;
        Ok(0)
    }
}
