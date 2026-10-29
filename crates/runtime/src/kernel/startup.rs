//! Process identity, startup records, environment and locale calls.
use super::*;

impl Kernel {
    pub(super) fn startup<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let result = match call {
            Call::GetVersion => 0x0893_0005,
            Call::GetCurrentProcessId => 1,
            Call::GetCurrentProcess => u32::MAX,
            Call::GetCurrentThread => u32::MAX - 1,
            Call::GetStartupInfoA => {
                let mut bytes = [0; 68];
                bytes[..4].copy_from_slice(&68u32.to_le_bytes());
                process.write(args[0], &bytes)?;
                0
            }
            Call::GetCommandLineA => {
                if let Some(address) = self.command_line {
                    address
                } else {
                    let value = b"C:\\Sims\\Sims.exe\0";
                    let address = process.heap_alloc(value.len() as u32)?;
                    process.write(address, value)?;
                    self.command_line = Some(address);
                    address
                }
            }
            Call::GetEnvironmentStringsW
            | Call::GetEnvironmentStrings
            | Call::GetEnvironmentStringsA => {
                let address = process.heap_alloc(4)?;
                self.environment.insert(address);
                address
            }
            Call::FreeEnvironmentStringsW | Call::FreeEnvironmentStringsA => {
                if !self.environment.remove(&args[0]) {
                    return Ok(self.fail(87));
                }
                process.heap_free(args[0])?;
                1
            }
            Call::GetStdHandle => match args[0] as i32 {
                -10 => 0xf01,
                -11 => 0xf02,
                -12 => 0xf03,
                _ => {
                    self.fail(87);
                    u32::MAX
                }
            },
            Call::GetFileType => {
                if self.files.owns_handle(args[0]) {
                    1
                } else if (0xf01..=0xf03).contains(&args[0]) {
                    2
                } else {
                    self.fail(6)
                }
            }
            Call::SetHandleCount => args[0],
            Call::GetACP => 1252,
            Call::GetOEMCP => 850,
            Call::GetUserDefaultLCID => 0x409,
            Call::IsValidCodePage => u32::from(matches!(args[0], 0 | 1252)),
            Call::GetCPInfo => {
                if !matches!(args[0], 0 | 1252) {
                    return Ok(self.fail(87));
                }
                let mut info = [0; 20];
                info[..4].copy_from_slice(&1u32.to_le_bytes());
                info[4] = b'?';
                process.write(args[1], &info)?;
                1
            }
            Call::WideCharToMultiByte | Call::MultiByteToWideChar => {
                let wide = call == Call::WideCharToMultiByte;
                if !wide && (args[1] & !0x0f != 0 || args[1] & 3 == 3) {
                    return Ok(self.fail(1004)); // ERROR_INVALID_FLAGS
                }
                if wide && !text::valid_wide_flags(args[1]) {
                    return Ok(self.fail(1004));
                }
                let count = text::convert(process, args, wide)?;
                if count == 0 {
                    self.fail(122);
                }
                count
            }
            Call::SetErrorMode => std::mem::replace(&mut self.error_mode, args[0]),
            Call::GetVersionExA => {
                let mut bytes = [0; 148];
                process.read(args[0], &mut bytes[..4])?;
                if u32::from_le_bytes(bytes[..4].try_into().unwrap()) < 148 {
                    return Ok(self.fail(87));
                }
                for (offset, value) in [(4, 5u32), (8, 0), (12, 2195), (16, 2)] {
                    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
                }
                process.write(args[0], &bytes)?;
                1
            }
            _ => return Err(Error::Host("incorrect kernel service routing".into())),
        };
        Ok(result)
    }
}
