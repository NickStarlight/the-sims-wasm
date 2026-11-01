//! System information and bounded diagnostic services for the guest process.
use super::*;

impl Kernel {
    pub(super) fn system_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        match call {
            Call::GlobalMemoryStatus => {
                // The guest's configured memory budget is its machine capacity;
                // browser/OS physical memory is neither available nor relevant.
                let total = process.address_limit().min(u64::from(u32::MAX));
                let used: u64 = process.regions().map(|r| u64::from(r.size)).sum();
                let available = total.saturating_sub(used);
                let load = 100 - available * 100 / total.max(1);
                let fields = [
                    32,
                    load as u32,
                    total as u32,
                    available as u32,
                    total as u32,
                    available as u32,
                    total as u32,
                    available as u32,
                ];
                let bytes: Vec<_> = fields.iter().flat_map(|v| v.to_le_bytes()).collect();
                process.write(args[0], &bytes)?;
                Ok(0)
            }
            Call::GetEnvironmentVariableA => {
                if args[0] == 0 || process.read_c_string(args[0], 32768)?.is_empty() {
                    return Ok(self.fail(87));
                }
                // Match GetEnvironmentStrings: guest environment starts empty.
                // Do not expose the browser or build machine's environment.
                Ok(self.fail(203))
            }
            Call::FormatMessageA => self.format_message(process, args),
            Call::ShellExecuteExA => {
                if args[0] == 0 {
                    return Ok(self.fail(87));
                }
                let mut size = [0; 4];
                process.read(args[0], &mut size)?;
                if u32::from_le_bytes(size) < 60 {
                    return Ok(self.fail(87));
                }
                // External processes and document handlers are unavailable.
                process.write(args[0] + 32, &31u32.to_le_bytes())?; // SE_ERR_NOASSOC
                process.write(args[0] + 56, &0u32.to_le_bytes())?; // hProcess
                Ok(self.fail(1155)) // ERROR_NO_ASSOCIATION
            }
            _ => Err(Error::Host("incorrect system service routing".into())),
        }
    }

    fn format_message<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        a: &[u32],
    ) -> Result<u32> {
        let flags = a[0];
        let source_flags = flags & 0x1c00;
        if flags & !0x3fff != 0
            || a[4] == 0
            || source_flags == 0
            || source_flags & 0x400 != 0 && source_flags != 0x400
        {
            return Ok(self.fail(87));
        }
        // Resource message tables are not implemented. Preserve the documented
        // failure so the game's diagnostic formatter can use its fallback.
        if source_flags == 0x800 {
            return Ok(self.fail(1813));
        }
        let mut output = if source_flags == 0x400 {
            if a[1] == 0 {
                return Ok(self.fail(87));
            }
            process.read_c_string(a[1], 65536)?
        } else {
            let message = match a[2] {
                0 => "The operation completed successfully.\r\n",
                2 => "The system cannot find the file specified.\r\n",
                3 => "The system cannot find the path specified.\r\n",
                5 => "Access is denied.\r\n",
                6 => "The handle is invalid.\r\n",
                8 => "Not enough memory resources are available to process this command.\r\n",
                87 => "The parameter is incorrect.\r\n",
                122 => "The data area passed to a system call is too small.\r\n",
                126 => "The specified module could not be found.\r\n",
                127 => "The specified procedure could not be found.\r\n",
                183 => "Cannot create a file when that file already exists.\r\n",
                203 => "The system could not find the environment option that was entered.\r\n",
                288 => "Attempt to release mutex not owned by caller.\r\n",
                _ => return Ok(self.fail(317)), // ERROR_MR_MID_NOT_FOUND
            };
            message.as_bytes().to_vec()
        };
        // No caller may silently get an incorrect expansion of insert arguments
        // or line wrapping. The game uses IGNORE_INSERTS and a fixed buffer.
        if flags & 0xff != 0 || flags & 0x200 == 0 && output.contains(&b'%') {
            return Ok(self.fail(87));
        }
        if a[3] != 0 && a[3] != 0x409 {
            return Ok(self.fail(1815));
        }
        let length = output.len() as u32;
        output.push(0);
        if flags & 0x100 != 0 {
            let size = (output.len() as u32).max(a[5]);
            if size > 65536 {
                return Ok(self.fail(234));
            }
            let address = process.heap_alloc(size)?;
            process.write(address, &output)?;
            if let Err(error) = process.write(a[4], &address.to_le_bytes()) {
                process.heap_free(address)?;
                return Err(error);
            }
        } else {
            if output.len() > a[5] as usize {
                return Ok(self.fail(122));
            }
            process.write(a[4], &output)?;
        }
        Ok(length)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/system.rs"]
mod tests;
