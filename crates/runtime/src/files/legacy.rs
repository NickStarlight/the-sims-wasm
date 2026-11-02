//! Legacy OpenFile handles share the CreateFile/GetFileSize/CloseHandle namespace.
//! The game's music loader uses this size probe to select streaming playback.
//! Returning HFILE_ERROR unconditionally sends MP3s to its zero-length PCM path.
use super::*;

impl Files {
    pub(super) fn open_legacy<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        let [name, output, style] = [args[0], args[1], args[2]];
        if output == 0 {
            return Ok((u32::MAX, 87));
        }
        let source = if style & 0x8000 != 0 {
            output + 8
        } else {
            name
        };
        let path = self.cased_path(process, source)?;
        let absolute = paths::absolute(&path);
        let bytes: Vec<u8> = absolute.chars().map(|c| c as u8).collect();
        let mut record = [0; 136]; // OFSTRUCT, including its 128-byte pathname.
        record[0] = 136;
        record[1] = 1; // The selected game directory is on a fixed disk.
        let error = if bytes.len() >= 128 {
            206
        } else if path.is_empty() || path.contains(['*', '?']) {
            123
        } else if style & !0xfb73 != 0 || style & 3 == 3 || style & 0x70 > 0x40 {
            87
        } else {
            0
        };
        if error == 0 {
            record[8..8 + bytes.len()].copy_from_slice(&bytes);
        }
        record[2..4].copy_from_slice(&(error as u16).to_le_bytes());
        process.write(output, &record)?;
        if error != 0 {
            return Ok((u32::MAX, error));
        }
        // Use the resolved path in OFSTRUCT for both ordinary and OF_REOPEN calls.
        let result = if style & 0x100 != 0 {
            (0, 0) // OF_PARSE does not open or require an existing file.
        } else if style & 0x200 != 0 {
            let (deleted, error) = self.call(process, "DeleteFileA", &[output + 8])?;
            (if deleted != 0 { 1 } else { u32::MAX }, error)
        } else {
            let access = if style & 0x1000 != 0 {
                0xc0000000 // OF_CREATE creates/truncates a read/write file.
            } else {
                [0x80000000, 0x40000000, 0xc0000000][(style & 3) as usize]
            };
            let share = match style & 0x70 {
                0x10 => 0,
                0x20 => 1,
                0x30 => 2,
                _ => 3,
            };
            let creation = if style & 0x1000 != 0 { 2 } else { 3 };
            let (handle, error) =
                self.open(process, &[output + 8, access, share, 0, creation, 0, 0])?;
            if handle != u32::MAX && style & 0x4000 != 0 {
                self.handles.remove(&handle); // OF_EXIST probes without leaking a handle.
            }
            (handle, if handle == u32::MAX { error } else { 0 })
        };
        process.write(output + 2, &(result.1 as u16).to_le_bytes())?;
        Ok(result)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/files/legacy.rs"]
mod tests;
