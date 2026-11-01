//! Cursor-based file I/O; guest-buffer failures do not advance file positions.
use super::*;
impl Files {
    pub(super) fn transfer<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        let failure = if matches!(name, "GetFileSize" | "SetFilePointer") {
            u32::MAX
        } else {
            0
        };
        let Some(file) = self.handles.get_mut(&args[0]) else {
            return Ok((failure, 6));
        };
        let linked = self
            .entries
            .get(&file.path)
            .is_some_and(|node| Rc::ptr_eq(node, &file.node));
        match name {
            "GetFileSize" => {
                let size = file.node.borrow().size();
                write_u32(process, args[1], (size >> 32) as u32)?;
                Ok((size as u32, 0))
            }
            "SetFilePointer" => {
                let distance = if args[2] == 0 {
                    i64::from(args[1] as i32)
                } else {
                    (i64::from(read_u32(process, args[2])? as i32) << 32) | i64::from(args[1])
                };
                let base = match args[3] {
                    0 => 0,
                    1 => file.position,
                    2 => file.node.borrow().size(),
                    _ => return Ok((u32::MAX, 87)),
                };
                let Some(position) = base.checked_add_signed(distance) else {
                    return Ok((u32::MAX, 131));
                };
                write_u32(process, args[2], (position >> 32) as u32)?;
                file.position = position;
                Ok((position as u32, 0))
            }
            "ReadFile" => {
                if !file.readable {
                    return Ok((0, 5));
                }
                if args[4] != 0 || args[3] == 0 {
                    return Ok((0, 87));
                }
                write_u32(process, args[3], 0)?;
                let node = file.node.borrow();
                let bytes = node.read(file.position, args[2] as usize)?;
                let length = bytes.len();
                process.write(args[1], &bytes)?;
                write_u32(process, args[3], length as u32)?;
                file.position += length as u64;
                Ok((1, 0))
            }
            "WriteFile" => {
                if !file.writable {
                    return Ok((0, 5));
                }
                if args[4] != 0 || args[3] == 0 {
                    return Ok((0, 87));
                }
                write_u32(process, args[3], 0)?;
                if args[2] == 0 {
                    return Ok((1, 0));
                }
                let mut bytes = Vec::new();
                if bytes.try_reserve_exact(args[2] as usize).is_err() {
                    return Ok((0, 8));
                }
                bytes.resize(args[2] as usize, 0);
                process.read(args[1], &mut bytes)?;
                let Some(end) = file
                    .position
                    .checked_add(bytes.len() as u64)
                    .and_then(|n| usize::try_from(n).ok())
                else {
                    return Ok((0, 87));
                };
                let mut node = file.node.borrow_mut();
                let target = node.writable()?;
                if end > target.len() {
                    if target.try_reserve(end - target.len()).is_err() {
                        return Ok((0, 8));
                    }
                    target.resize(end, 0);
                }
                target[file.position as usize..end].copy_from_slice(&bytes);
                let change = journal::Change::Write {
                    path: file.path.clone(),
                    offset: file.position as usize,
                    size: target.len(),
                };
                file.position = end as u64;
                write_u32(process, args[3], args[2])?;
                drop(node);
                if linked {
                    self.record(change, &bytes)?;
                }
                Ok((1, 0))
            }
            "SetEndOfFile" => {
                if !file.writable {
                    return Ok((0, 5));
                }
                let Ok(size) = usize::try_from(file.position) else {
                    return Ok((0, 87));
                };
                let mut node = file.node.borrow_mut();
                let target = node.writable()?;
                if size > target.len() && target.try_reserve(size - target.len()).is_err() {
                    return Ok((0, 8));
                }
                target.resize(size, 0);
                let change = journal::Change::Resize {
                    path: file.path.clone(),
                    size,
                };
                drop(node);
                if linked {
                    self.record(change, &[])?;
                }
                Ok((1, 0))
            }
            _ => Err(Error::Host(format!("unknown file transfer {name}"))),
        }
    }
}
