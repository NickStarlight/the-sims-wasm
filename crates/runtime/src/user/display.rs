//! DEVMODEA records for the process's virtual display; no physical monitor changes.
use super::*;
const LEGACY_SIZE: usize = 124;
const FULL_SIZE: usize = 156;
impl User {
    pub(super) fn change_display_settings<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        // Only the process's virtual display changes; no host monitor settings.
        if a[1] & !(2 | 4 | 0x40000000) != 0 {
            return Ok((-4i32) as u32); // DISP_CHANGE_BADFLAGS
        }
        let mode = if a[0] == 0 {
            crate::display::DEFAULT_MODE
        } else {
            let mut header = [0; 40];
            process.read(a[0], &mut header)?;
            if u16::from_le_bytes([header[36], header[37]]) < LEGACY_SIZE as u16 {
                return Ok((-5i32) as u32); // DISP_CHANGE_BADPARAM
            }
            let mut bytes = [0; LEGACY_SIZE];
            process.read(a[0], &mut bytes)?;
            let word = |offset| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            let fields = word(40);
            let (mut width, mut height, mut bits) = self.display.mode();
            if fields & 0x80000 != 0 {
                width = word(108);
            }
            if fields & 0x100000 != 0 {
                height = word(112);
            }
            if fields & 0x40000 != 0 {
                bits = word(104);
            }
            if !crate::display::MODES
                .iter()
                .any(|m| m.0 == width && m.1 == height)
                || ![16, 32].contains(&bits)
                || (fields & 0x200000 != 0 && word(116) != 0)
                || (fields & 0x400000 != 0 && ![0, 1, 60].contains(&word(120)))
            {
                return Ok((-2i32) as u32); // DISP_CHANGE_BADMODE
            }
            (width, height, bits)
        };
        if a[1] & 2 == 0 && mode != self.display.mode() {
            self.display.set_mode(mode);
            let windows: Vec<_> = self.windows.keys().copied().collect();
            for window in windows {
                self.enqueue(Message::new(window, 0x7e, mode.2, mode.0 | (mode.1 << 16)))?;
            }
        }
        Ok(0)
    }

    pub(super) fn enum_display_settings<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        if a[2] == 0 {
            return Ok(self.fail(87));
        }
        if a[0] != 0 {
            let device = process.read_c_string(a[0], 256)?;
            if !device.is_empty()
                && !device.eq_ignore_ascii_case(b"\\\\.\\DISPLAY1")
                && !device.eq_ignore_ascii_case(b"display")
            {
                return Ok(self.fail(87));
            }
        }
        let mode = match a[1] {
            u32::MAX => self.display.mode(),
            0xffff_fffe => crate::display::DEFAULT_MODE,
            index => match crate::display::MODES.get(index as usize) {
                Some(&mode) => mode,
                None => return Ok(0),
            },
        };
        // The 124-byte Win95 layout ends after frequency; modern callers use 156 bytes.
        // Honor the supplied public size and leave any driver-extra bytes untouched.
        let mut header = [0; 40];
        process.read(a[2], &mut header)?;
        let size = u16::from_le_bytes([header[36], header[37]]) as usize;
        if size < LEGACY_SIZE {
            return Ok(self.fail(87));
        }
        let size = size.min(FULL_SIZE);
        let mut record = vec![0; size];
        record[..7].copy_from_slice(b"display");
        record[32..34].copy_from_slice(&0x401u16.to_le_bytes());
        record[34..36].copy_from_slice(&1u16.to_le_bytes());
        record[36..38].copy_from_slice(&(size as u16).to_le_bytes());
        for (offset, value) in [
            (40, 0x007c0000),
            (104, mode.2),
            (108, mode.0),
            (112, mode.1),
            (116, 0),
            (120, 60),
        ] {
            record[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
        }
        process.write(a[2], &record)?;
        Ok(1)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/user/display.rs"]
mod tests;
