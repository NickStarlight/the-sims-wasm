//! Key creation dispositions, root semantics, and nonrecursive deletion.
use super::*;
impl Registry {
    pub(super) fn key_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        if name == "RegCloseKey" {
            return Ok(
                if ROOTS.iter().any(|(id, _)| *id == args[0])
                    || self.handles.remove(&args[0]).is_some()
                {
                    0
                } else {
                    INVALID_HANDLE
                },
            );
        }
        let base = match self.handle(args[0]) {
            Ok(value) => value,
            Err(status) => return Ok(status),
        };
        let sub = string(process, args[1])?;
        let path = joined(&base.path, &sub);
        if name == "RegDeleteKeyA" {
            if sub.is_empty() {
                return Ok(87);
            }
            let Some(key) = self.keys.get(&path) else {
                return Ok(2);
            };
            let prefix = format!("{path}\\");
            if self.keys.keys().any(|name| name.starts_with(&prefix)) {
                return Ok(5);
            }
            key.borrow_mut().deleted = true;
            self.keys.remove(&path);
            return Ok(0);
        }
        let create = matches!(name, "RegCreateKeyExA" | "RegCreateKeyA");
        let (out, disposition, access) = match name {
            "RegOpenKeyExA" => {
                if args[2] != 0 {
                    return Ok(87);
                }
                (args[4], 0, args[3])
            }
            "RegCreateKeyExA" => {
                if args[1] == 0 || args[2] != 0 || args[4] > 1 {
                    return Ok(87);
                }
                (args[7], args[8], args[5])
            }
            _ => (args[2], 0, ALL_ACCESS),
        };
        if out == 0 {
            return Ok(87);
        }
        let existed = self.keys.contains_key(&path);
        if !existed && !create {
            return Ok(2);
        }
        // Validate writable result buffers before allocating a handle or creating keys.
        write_u32(process, out, 0)?;
        write_u32(process, disposition, 0)?;
        if !create && path == base.path && ROOTS.iter().any(|(id, _)| *id == args[0]) {
            write_u32(process, out, args[0])?;
            write_u32(process, disposition, 2)?;
            return Ok(0);
        }
        let key = if existed {
            self.keys[&path].clone()
        } else {
            self.create_path(&path)
        };
        let access = if access & 0x02000000 != 0 {
            ALL_ACCESS
        } else {
            access
        };
        let id = self.allocate_handle(Handle { path, key, access })?;
        write_u32(process, out, id)?;
        write_u32(process, disposition, if existed { 2 } else { 1 })?;
        Ok(0)
    }
}
