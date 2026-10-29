//! Bounded scratch buffers for C memory operations, including overlapping moves.
use super::*;

pub(super) fn call<M: GuestMemory>(
    process: &mut Process<M>,
    call: Call,
    args: &[u32],
) -> Result<u32> {
    let [destination, source, count] = [args[0], args[1], args[2]];
    if count == 0 {
        return Ok(if call == Call::Memcmp { 0 } else { destination });
    }
    let mut scratch = [0; 4096];
    let mut remaining = count;
    while remaining != 0 {
        let length = remaining.min(scratch.len() as u32);
        let backwards = matches!(call, Call::Memcpy | Call::Memmove) && destination > source;
        let offset = if backwards {
            remaining - length
        } else {
            count - remaining
        };
        let target = destination.checked_add(offset).ok_or(Error::InvalidRange)?;
        let chunk = &mut scratch[..length as usize];
        if call == Call::Memset {
            chunk.fill(source as u8);
        } else {
            let address = source.checked_add(offset).ok_or(Error::InvalidRange)?;
            process.read(address, chunk)?;
        }
        if call == Call::Memcmp {
            let mut left = [0; 4096];
            process.read(target, &mut left[..length as usize])?;
            for (&a, &b) in left[..length as usize].iter().zip(chunk.iter()) {
                if a != b {
                    return Ok((i32::from(a) - i32::from(b)) as u32);
                }
            }
        } else {
            process.write(target, chunk)?;
        }
        remaining -= length;
    }
    Ok(if call == Call::Memcmp { 0 } else { destination })
}
