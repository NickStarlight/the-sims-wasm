//! Palette entries and window clipper ownership.
use super::*;
impl Graphics {
    pub(super) fn create_palette<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        modules: &ModuleCatalog,
        a: &[u32],
    ) -> Result<u32> {
        if a[3] == 0 || a[4] != 0 {
            return Ok(INVALID);
        }
        let count = match a[1] & 0x305 {
            0x100 => 2,
            0x200 => 4,
            1 => 16,
            4 => 256,
            _ => return Ok(INVALID),
        };
        let mut bytes = [0; 1024];
        if a[2] != 0 {
            p.read(a[2], &mut bytes[..count * 4])?;
        }
        let mut entries = [[0; 4]; 256];
        for (entry, bytes) in entries.iter_mut().zip(bytes.as_chunks::<4>().0.iter()) {
            entry.copy_from_slice(bytes);
        }
        let id = self.object(p, modules, Kind::Palette)?;
        self.palettes.insert(
            id,
            Palette {
                flags: a[1],
                count,
                entries,
            },
        );
        write_u32(p, a[3], id)?;
        Ok(0)
    }
    pub(super) fn palette_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        let Some(palette) = self.palettes.get_mut(&a[0]) else {
            return Ok(INVALID);
        };
        if name == "Palette_Initialize" {
            return Ok(0x88760005);
        }
        if name == "Palette_GetCaps" {
            if a[1] != 0 {
                write_u32(p, a[1], palette.flags)?;
            }
            return Ok(0);
        }
        if !matches!(name, "Palette_GetEntries" | "Palette_SetEntries") {
            return Ok(NOT_IMPLEMENTED);
        }
        let Some(end) = a[2]
            .checked_add(a[3])
            .filter(|&end| end as usize <= palette.count)
        else {
            return Ok(INVALID);
        };
        if a[4] == 0 || a[2] as usize >= palette.count {
            return Ok(INVALID);
        }
        let entries = &mut palette.entries[a[2] as usize..end as usize];
        if name == "Palette_GetEntries" {
            let bytes: Vec<_> = entries.iter().flatten().copied().collect();
            p.write(a[4], &bytes)?;
        } else {
            let mut bytes = vec![0; entries.len() * 4];
            p.read(a[4], &mut bytes)?;
            for (entry, bytes) in entries.iter_mut().zip(bytes.as_chunks::<4>().0.iter()) {
                entry.copy_from_slice(bytes);
            }
            let primary = self
                .surfaces
                .iter()
                .find(|(_, s)| s.palette == a[0] && s.primary)
                .map(|(&id, _)| id);
            if let Some(id) = primary {
                self.present(id)?;
            }
        }
        Ok(0)
    }
}
