//! Query sizing and stored bytes follow Win32 contracts, including short-buffer results.
use super::*;
impl Registry {
    pub(super) fn value_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        let mut handle = match self.handle(args[0]) {
            Ok(value) => value,
            Err(status) => return Ok(status),
        };
        if matches!(name, "RegQueryValueA" | "RegSetValueA") {
            let path = joined(&handle.path, &string(process, args[1])?);
            if name == "RegSetValueA" {
                if args[2] != 1 {
                    return Ok(87);
                } // legacy API only stores the default REG_SZ value
                if handle.access & 2 == 0 {
                    return Ok(5);
                }
                handle.key = self.create_path(&path);
                let mut bytes = if args[3] == 0 {
                    Vec::new()
                } else {
                    process.read_c_string(args[3], 32768)?
                };
                bytes.push(0);
                handle
                    .key
                    .borrow_mut()
                    .values
                    .insert(String::new(), Value { kind: 1, bytes });
                return Ok(0);
            }
            let Some(key) = self.keys.get(&path) else {
                return Ok(2);
            };
            handle.key = key.clone();
            return query(process, &handle, "", 0, args[2], args[3]);
        }
        let value_name = string(process, args[1])?.to_lowercase();
        match name {
            "RegQueryValueExA" => {
                if args[2] != 0 {
                    return Ok(87);
                }
                query(process, &handle, &value_name, args[3], args[4], args[5])
            }
            "RegSetValueExA" => {
                if args[2] != 0 || (args[4] == 0 && args[5] != 0) {
                    return Ok(87);
                }
                if handle.access & 2 == 0 {
                    return Ok(5);
                }
                let mut bytes = Vec::new();
                if bytes.try_reserve_exact(args[5] as usize).is_err() {
                    return Ok(8);
                }
                bytes.resize(args[5] as usize, 0);
                if !bytes.is_empty() {
                    process.read(args[4], &mut bytes)?;
                }
                handle.key.borrow_mut().values.insert(
                    value_name,
                    Value {
                        kind: args[3],
                        bytes,
                    },
                );
                Ok(0)
            }
            "RegDeleteValueA" => {
                if handle.access & 2 == 0 {
                    return Ok(5);
                }
                Ok(
                    if handle.key.borrow_mut().values.remove(&value_name).is_some() {
                        0
                    } else {
                        2
                    },
                )
            }
            _ => Err(Error::Host(format!("unknown registry value API {name}"))),
        }
    }
}
fn query<M: GuestMemory>(
    process: &mut Process<M>,
    handle: &Handle,
    name: &str,
    kind: u32,
    data: u32,
    size: u32,
) -> Result<u32> {
    if data != 0 && size == 0 {
        return Ok(87);
    }
    if handle.access & 1 == 0 {
        return Ok(5);
    }
    let key = handle.key.borrow();
    let Some(value) = key.values.get(name) else {
        return Ok(2);
    };
    let capacity = if data != 0 {
        read_u32(process, size)?
    } else {
        0
    };
    write_u32(process, kind, value.kind)?;
    write_u32(process, size, value.bytes.len() as u32)?;
    if data != 0 {
        if capacity < value.bytes.len() as u32 {
            return Ok(234);
        }
        process.write(data, &value.bytes)?;
    }
    Ok(0)
}
