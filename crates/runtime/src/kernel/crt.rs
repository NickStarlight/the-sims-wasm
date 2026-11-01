//! Cdecl allocation and byte/string operations; guest addresses remain checked integers.
mod bytes;
mod strings;
use super::Call;
use crate::{Error, GuestMemory, Process, Result};

pub(super) fn call<M: GuestMemory>(
    process: &mut Process<M>,
    call: Call,
    args: &[u32],
) -> Result<u32> {
    match call {
        Call::Malloc => allocation(process.heap_alloc(args[0])),
        Call::Free => {
            process.heap_free(args[0])?;
            Ok(0)
        }
        Call::Realloc => {
            if args[1] == 0 {
                process.heap_free(args[0])?;
                Ok(0)
            } else {
                allocation(process.heap_realloc(args[0], args[1]))
            }
        }
        Call::Calloc => {
            let Some(size) = args[0].checked_mul(args[1]) else {
                return Ok(0);
            };
            // Process heap allocations are zero-filled, including reused blocks.
            allocation(process.heap_alloc(size))
        }
        Call::Memcpy | Call::Memmove | Call::Memset | Call::Memcmp => {
            bytes::call(process, call, args)
        }
        _ => strings::call(process, call, args),
    }
}
fn allocation(result: Result<u32>) -> Result<u32> {
    match result {
        Err(Error::OutOfMemory) => Ok(0),
        other => other,
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/crt.rs"]
mod tests;
