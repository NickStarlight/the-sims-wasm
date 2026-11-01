//! Search the virtual desktop's existing windows; no host desktop windows are fabricated.
use super::*;

impl User {
    pub(super) fn find_window<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        args: &[u32],
    ) -> Result<u32> {
        let [parent, after, class, title] = [args[0], args[1], args[2], args[3]];
        if parent == 0xffff_fffd {
            return Err(Error::Host("message-only windows are unsupported".into()));
        }
        if parent != 0 && !self.windows.contains_key(&parent) {
            return Ok(self.fail(1400));
        }
        if after != 0
            && !self
                .windows
                .get(&after)
                .is_some_and(|window| window.parent == parent)
        {
            return Ok(self.fail(1400));
        }
        let name = if class > 0xffff {
            Some(process.read_c_string(class, 256)?)
        } else {
            None
        };
        let title = if title != 0 {
            Some(process.read_c_string(title, 65536)?)
        } else {
            None
        };
        // Creation places new windows at the top of this virtual desktop's order.
        // No SetWindowPos/reordering service is currently implemented.
        for (&handle, window) in self.windows.iter().rev() {
            if window.parent != parent || (after != 0 && handle >= after) {
                continue;
            }
            let class_matches = if let Some(name) = &name {
                equal_ansi(&window.class_name, name)
            } else {
                class == 0 || class == window.class_atom
            };
            if class_matches
                && title
                    .as_ref()
                    .is_none_or(|title| equal_ansi(&window.title, title))
            {
                return Ok(handle);
            }
        }
        Ok(0)
    }
}

fn equal_ansi(left: &[u8], right: &[u8]) -> bool {
    // Windows-1252 simple case pairs; avoid assuming UTF-8 for guest ANSI bytes.
    fn lower(byte: u8) -> u8 {
        match byte {
            b'A'..=b'Z' | 0xc0..=0xd6 | 0xd8..=0xde => byte + 0x20,
            0x8a => 0x9a,
            0x8c => 0x9c,
            0x8e => 0x9e,
            0x9f => 0xff,
            _ => byte,
        }
    }
    left.len() == right.len() && left.iter().zip(right).all(|(&a, &b)| lower(a) == lower(b))
}
