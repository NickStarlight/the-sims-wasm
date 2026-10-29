//! Legacy surface duplication, depth attachment and key contracts over owned records.
use super::*;
impl Graphics {
    pub(super) fn duplicate_surface<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        a: &[u32],
    ) -> Result<u32> {
        if a[2] == 0 {
            return Ok(0);
        }
        write_u32(p, a[2], 0)?;
        let Some(mut copy) = self.surfaces.get(&a[1]).cloned() else {
            return Ok(0);
        };
        // Legacy DuplicateSurface allocates fresh zeroed pixels, retaining descriptor/key metadata.
        copy.pixels = Rc::new(zero_bytes(copy.pixels.len())?);
        copy.guest_pixels = 0;
        copy.guest_pixels_current = false;
        copy.dc = 0;
        copy.lock = None;
        copy.primary = false;
        copy.back = 0;
        copy.front = 0;
        let id = self.object(p, modules, Kind::Surface)?;
        for child in [copy.depth, copy.palette, copy.clipper] {
            self.add_ref(child);
        }
        self.surfaces.insert(id, copy);
        write_u32(p, a[2], id)?;
        Ok(0)
    }
    pub(super) fn attached_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        if name == "Surface_DeleteAttachedSurface" {
            if a[2] == 0 || self.surfaces[&a[0]].depth != a[2] {
                return Ok(0x887600ff);
            }
            self.surfaces.get_mut(&a[0]).unwrap().depth = 0;
            self.release(p, a[2])?;
            return Ok(0);
        }
        let surface = &self.surfaces[&a[0]];
        let Some(other) = self.surfaces.get(&a[1]) else {
            return Ok(INVALID);
        };
        if a[0] == a[1]
            || other.caps & 0x20000 == 0
            || other.format.bits != 16
            || other
                .pixel_format
                .is_none_or(|pf| u32::from_le_bytes(pf[4..8].try_into().unwrap()) & 0x400 == 0)
            || surface.depth != 0
            || surface.width != other.width
            || surface.height != other.height
        {
            return Ok(INVALID);
        }
        self.surfaces.get_mut(&a[0]).unwrap().depth = a[1];
        self.add_ref(a[1]);
        Ok(0)
    }
    pub(super) fn color_key<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        a: &[u32],
        set: bool,
    ) -> Result<u32> {
        let mut flags = a[1];
        let value = if !set || a[2] == 0 {
            None
        } else {
            Some((read_u32(p, a[2])?, read_u32(p, a[2] + 4)?))
        };
        if set && flags & 1 != 0 {
            if value.is_some_and(|(lo, hi)| lo != hi) {
                return Ok(0x887600dc);
            }
            flags &= !1;
        }
        let surface = self.surfaces.get_mut(&a[0]).unwrap();
        let key = match flags {
            8 => &mut surface.source_key,
            2 => &mut surface.destination_key,
            16 => &mut surface.source_overlay_key,
            4 => &mut surface.destination_overlay_key,
            _ => return Ok(if set { INVALID } else { 0x887600d7 }),
        };
        if set {
            *key = value;
        } else {
            if a[2] == 0 {
                return Ok(INVALID);
            }
            let Some((low, high)) = *key else {
                return Ok(0x887600d7);
            };
            write_u32(p, a[2], low)?;
            write_u32(p, a[2] + 4, high)?;
        }
        Ok(0)
    }
}
