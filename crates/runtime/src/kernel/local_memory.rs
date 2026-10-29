//! Fixed Local/Global objects share the process heap; movable handles are unsupported.
//! No allocation headers or hidden runtime pointers are written into guest memory.
use super::*;

impl Kernel {
    pub(super) fn local_memory<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let operation = match call {
            Call::LocalAlloc | Call::GlobalAlloc => {
                if args[0] & !0x40 != 0 {
                    return Err(Error::Host(format!(
                        "Local/GlobalAlloc supports fixed allocations only; flags={:#x}",
                        args[0]
                    )));
                }
                process.heap_alloc(args[1])
            }
            Call::LocalFree | Call::GlobalFree => {
                return match process.heap_free(args[0]) {
                    Ok(()) => Ok(0),
                    Err(Error::InvalidAllocation) => {
                        self.fail(6);
                        Ok(args[0])
                    }
                    Err(error) => Err(error),
                };
            }
            Call::LocalSize | Call::GlobalSize => process.heap_size(args[0]),
            Call::LocalLock | Call::GlobalLock => match process.heap_size(args[0]) {
                Ok(0) => return Ok(self.fail(157)),
                Ok(_) => Ok(args[0]),
                Err(error) => Err(error),
            },
            Call::LocalUnlock | Call::GlobalUnlock => {
                return Ok(self.fail(if process.heap_size(args[0]).is_ok() {
                    158
                } else {
                    6
                }));
            }
            Call::LocalReAlloc | Call::GlobalReAlloc => {
                if args[2] & !0x42 != 0 || args[1] == 0 {
                    return Err(Error::Host("Local/GlobalReAlloc attribute modification and zero-size discard are unsupported".into()));
                }
                match process.heap_size(args[0]) {
                    // A larger actual allocation is legal. Preserve its address and bytes.
                    Ok(size) if args[1] <= size => Ok(args[0]),
                    // MOVEABLE on reallocation permits relocation of an existing fixed block;
                    // it does not turn that block into a movable handle.
                    Ok(_) if args[2] & 2 != 0 => process.heap_realloc(args[0], args[1]),
                    Ok(_) => Err(Error::OutOfMemory),
                    Err(error) => Err(error),
                }
            }
            _ => return Err(Error::Host("incorrect local-memory service routing".into())),
        };
        match operation {
            Ok(value) => Ok(value),
            Err(Error::InvalidAllocation) => Ok(self.fail(6)),
            Err(Error::OutOfMemory) => Ok(self.fail(8)),
            Err(error) => Err(error),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/local_memory.rs"]
mod tests;
