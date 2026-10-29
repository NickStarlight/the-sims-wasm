//! Heap and virtual-memory kernel calls.
use super::*;

impl Kernel {
    pub(super) fn memory<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let result = match call {
            Call::GetProcessHeap => 1,
            Call::HeapCreate => {
                let handle = self.heaps.len() as u32 + 1;
                self.heaps.insert(handle);
                handle
            }
            Call::HeapAlloc => {
                self.heap(args[0])?;
                process.heap_alloc(args[2])?
            }
            Call::HeapFree => {
                self.heap(args[0])?;
                process.heap_free(args[2])?;
                1
            }
            Call::HeapSize => {
                self.heap(args[0])?;
                process.heap_size(args[2])?
            }
            Call::HeapReAlloc => {
                self.heap(args[0])?;
                if args[1] & 0x10 != 0 {
                    return Err(Error::Host("in-place HeapReAlloc is unsupported".into()));
                }
                process.heap_realloc(args[2], args[3])?
            }
            Call::VirtualAlloc => {
                process.virtual_alloc_flags(args[0], args[1], args[2], args[3])?
            }
            Call::VirtualFree => {
                let result = match args[2] {
                    0x4000 => process.virtual_decommit(args[0], args[1]),
                    0x8000 if args[1] == 0 => process.virtual_free(args[0]),
                    _ => return Ok(self.fail(87)),
                };
                if result.is_err() {
                    return Ok(self.fail(487));
                }
                1
            }
            _ => return Err(Error::Host("incorrect kernel service routing".into())),
        };
        Ok(result)
    }
}
