//! Module APIs resolve only loader-installed images and configured native bindings.
use super::*;
use crate::modules::Symbol;

impl Kernel {
    /// Read-only boot linker lookup; no guest allocations, reference changes, or guest callbacks.
    pub fn export_address(
        &self,
        dll: &str,
        name: Option<&str>,
        ordinal: Option<u32>,
    ) -> Option<u32> {
        let symbol = match (name, ordinal) {
            (Some(name), None) => Symbol::Name(name.to_owned()),
            (None, Some(ordinal)) => Symbol::Ordinal(ordinal),
            _ => return None,
        };
        let handle = self.modules.handle(Some(dll))?;
        self.modules.procedure(handle, symbol).ok()
    }

    pub(super) fn module_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let value = match call {
            Call::GetModuleHandleA | Call::GetModuleHandleW => {
                let name = if args[0] == 0 {
                    None
                } else {
                    Some(read_name(process, args[0], call == Call::GetModuleHandleW)?)
                };
                self.modules
                    .handle(name.as_deref())
                    .unwrap_or_else(|| self.fail(126))
            }
            Call::GetModuleFileNameA | Call::GetModuleFileNameW => {
                let Some(path) = self.modules.filename(args[0]) else {
                    return Ok(self.fail(6));
                };
                if args[2] == 0 {
                    return Ok(self.fail(122));
                }
                let units: Vec<_> = path.encode_utf16().collect();
                let count = units.len().min(args[2] as usize - 1);
                let truncated = units.len() >= args[2] as usize;
                let mut output = Vec::new();
                for unit in units[..count].iter().copied().chain(std::iter::once(0)) {
                    if call == Call::GetModuleFileNameW {
                        output.extend(unit.to_le_bytes());
                    } else {
                        output.push(text::encode_unit(unit).unwrap_or(b'?'));
                    }
                }
                process.write(args[1], &output)?;
                if truncated {
                    self.fail(122);
                    args[2]
                } else {
                    count as u32
                }
            }
            Call::LoadLibraryA | Call::LoadLibraryExA => {
                if args[0] == 0 {
                    return Ok(self.fail(87));
                }
                // All guest images are installed by the loader. Search-path-only
                // flags do not alter that catalog; data/resource-only loads need
                // a separate mapping and are not implemented here.
                if call == Call::LoadLibraryExA && (args[1] != 0 || args[2] & !0x8 != 0) {
                    return Ok(self.fail(87));
                }
                let name = read_name(process, args[0], false)?;
                self.modules.load(&name).unwrap_or_else(|| self.fail(126))
            }
            Call::FreeLibrary => {
                if self.modules.free(args[0]) {
                    1
                } else {
                    self.fail(6)
                }
            }
            Call::GetProcAddress => {
                if args[0] == 0 || self.modules.filename(args[0]).is_none() {
                    return Ok(self.fail(6));
                }
                let symbol = if args[1] <= u32::from(u16::MAX) {
                    Symbol::Ordinal(args[1])
                } else {
                    Symbol::Name(read_name(process, args[1], false)?)
                };
                match self.modules.procedure(args[0], symbol) {
                    Ok(address) => address,
                    Err(error) => self.fail(error),
                }
            }
            _ => return Err(Error::Host("incorrect module service routing".into())),
        };
        Ok(value)
    }
}

fn read_name<M: GuestMemory>(process: &Process<M>, address: u32, wide: bool) -> Result<String> {
    let width = if wide { 2 } else { 1 };
    let mut units = Vec::new();
    while units.len() < 32768 {
        let current = address
            .checked_add(units.len() as u32 * width)
            .ok_or(Error::InvalidRange)?;
        let mut bytes = [0; 256];
        let mut length = bytes.len().min((32768 - units.len()) * width as usize);
        match process.read(current, &mut bytes[..length]) {
            Ok(()) => {}
            // A terminator can precede an unreadable boundary. Read one unit only
            // there; ordinary names cross the Wasm/host memory bridge in one batch.
            Err(Error::Unmapped | Error::Protected | Error::InvalidRange) => {
                length = width as usize;
                process.read(current, &mut bytes[..length])?;
            }
            Err(error) => return Err(error),
        }
        for index in (0..length).step_by(width as usize) {
            let unit = if wide {
                u16::from_le_bytes([bytes[index], bytes[index + 1]])
            } else {
                text::decode_byte(bytes[index])
            };
            if unit == 0 {
                return Ok(String::from_utf16_lossy(&units));
            }
            units.push(unit);
        }
    }
    Err(Error::Host("unterminated module or procedure name".into()))
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/modules.rs"]
mod tests;
