//! Owned replacement for legacy surface slot17's fixed DC token and slot26's release.
//! The old path did not render GDI operations into the surface; neither does this adapter.
use super::*;
impl Graphics {
    pub(crate) fn surface_dc<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        user: &mut crate::user::User,
        name: &str,
        a: &[u32],
        thread: u32,
    ) -> Result<u32> {
        let Some(surface) = self.surfaces.get(&a[0]) else {
            return Ok(INVALID);
        };
        let current = surface.dc;
        if name == "Surface_GetDC" {
            if a[1] == 0 {
                return Ok(INVALID);
            }
            write_u32(p, a[1], 0)?;
            let dc = if current != 0 {
                current
            } else {
                let dc = match user.acquire_surface_dc(a[0], thread) {
                    Ok(dc) => dc,
                    Err(_) => return Ok(0x8007000e),
                };
                self.surfaces.get_mut(&a[0]).unwrap().dc = dc;
                // The live DC lease holds its surface alive until ReleaseDC.
                self.add_ref(a[0]);
                dc
            };
            write_u32(p, a[1], dc)?;
            Ok(0)
        } else {
            if current == 0 || current != a[1] || !user.release_surface_dc(a[0], a[1], thread) {
                return Ok(INVALID);
            }
            self.surfaces.get_mut(&a[0]).unwrap().dc = 0;
            self.release(p, a[0])?;
            Ok(0)
        }
    }
}
