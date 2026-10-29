//! Byte strings use unsigned comparisons; the initial C locale folds ASCII only.
use super::*;

const MAX_STRING: u32 = 1_048_576;
fn byte<M: GuestMemory>(process: &Process<M>, address: u32, offset: u32) -> Result<u8> {
    let mut value = [0];
    process.read(
        address.checked_add(offset).ok_or(Error::InvalidRange)?,
        &mut value,
    )?;
    Ok(value[0])
}
fn string<M: GuestMemory>(process: &Process<M>, address: u32, maximum: u32) -> Result<Vec<u8>> {
    let mut result = Vec::new();
    for offset in 0..maximum.min(MAX_STRING) {
        let value = byte(process, address, offset)?;
        result.push(value);
        if value == 0 {
            return Ok(result);
        }
    }
    if maximum > MAX_STRING {
        return Err(Error::Host("CRT string exceeds bounded scan limit".into()));
    }
    Ok(result)
}

pub(super) fn call<M: GuestMemory>(
    process: &mut Process<M>,
    call: Call,
    args: &[u32],
) -> Result<u32> {
    match call {
        Call::Strcmp | Call::Strncmp | Call::Stricmp => {
            let limit = if call == Call::Strncmp {
                args[2]
            } else {
                u32::MAX
            };
            for offset in 0..limit.min(MAX_STRING) {
                let mut a = byte(process, args[0], offset)?;
                let mut b = byte(process, args[1], offset)?;
                if call == Call::Stricmp {
                    a = a.to_ascii_lowercase();
                    b = b.to_ascii_lowercase();
                }
                if a != b {
                    return Ok((i32::from(a) - i32::from(b)) as u32);
                }
                if a == 0 {
                    return Ok(0);
                }
            }
            if limit > MAX_STRING {
                return Err(Error::Host(
                    "CRT comparison exceeds bounded scan limit".into(),
                ));
            }
            Ok(0)
        }
        Call::Strlen | Call::Strchr | Call::Strrchr => {
            let value = string(process, args[0], u32::MAX)?;
            if call == Call::Strlen {
                return Ok(value.len() as u32 - 1);
            }
            let needle = args[1] as u8;
            let position = if call == Call::Strrchr {
                value.iter().rposition(|byte| *byte == needle)
            } else {
                value.iter().position(|byte| *byte == needle)
            };
            position.map_or(Ok(0), |offset| {
                args[0]
                    .checked_add(offset as u32)
                    .ok_or(Error::InvalidRange)
            })
        }
        Call::Strcpy | Call::Strncpy => {
            let count = if call == Call::Strncpy {
                args[2]
            } else {
                u32::MAX
            };
            if count == 0 {
                return Ok(args[0]);
            }
            let value = string(process, args[1], count)?;
            process.write(args[0], &value)?;
            if call == Call::Strncpy && value.len() < count as usize {
                let start = args[0]
                    .checked_add(value.len() as u32)
                    .ok_or(Error::InvalidRange)?;
                super::bytes::call(
                    process,
                    Call::Memset,
                    &[start, 0, count - value.len() as u32],
                )?;
            }
            Ok(args[0])
        }
        _ => Err(Error::Host("unsupported CRT call".into())),
    }
}
