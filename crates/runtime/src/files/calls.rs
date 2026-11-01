//! File API dispatch returns Windows failure values plus the thread's last error.
use super::*;
impl Files {
    pub fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        match name {
            "CreateFileA" => self.open(process, args),
            "CopyFileA" | "MoveFileA" => self.copy_or_move(process, name, args),
            "FlushFileBuffers" => Ok(if self.handles.contains_key(&args[0]) {
                (1, 0)
            } else {
                (0, 6)
            }),
            "ReadFile" | "WriteFile" | "SetFilePointer" | "GetFileSize" | "SetEndOfFile" => {
                self.transfer(process, name, args)
            }
            "FindFirstFileA" | "FindNextFileA" | "FindClose" => self.search(process, name, args),
            "GetPrivateProfileStringA" | "GetPrivateProfileIntA" | "WritePrivateProfileStringA" => {
                self.profile(process, name, args)
            }
            "CloseHandle" => Ok(if self.handles.remove(&args[0]).is_some() {
                (1, 0)
            } else {
                (0, 6)
            }),
            "GetFileAttributesA" => {
                let path = self.path(process, args[0])?;
                Ok(if self.directories.contains(&path) {
                    (0x10, 0)
                } else if self.entries.contains_key(&path) {
                    (0x20, 0)
                } else {
                    (u32::MAX, 2)
                })
            }
            "DeleteFileA" => {
                let path = self.path(process, args[0])?;
                Ok(if self.entries.remove(&path).is_some() {
                    self.names.remove(&path);
                    self.record(journal::Change::Delete { path }, &[])?;
                    (1, 0)
                } else {
                    (0, 2)
                })
            }
            "CreateDirectoryA" => {
                let spelling = self.cased_path(process, args[0])?;
                let path = spelling.to_lowercase();
                if self.directories.contains(&path) || self.entries.contains_key(&path) {
                    return Ok((0, 183));
                }
                if !self.directories.contains(paths::parent(&path)) {
                    return Ok((0, 3));
                }
                self.directories.insert(path);
                self.remember_name(&spelling);
                Ok((1, 0))
            }
            "SetCurrentDirectoryA" => {
                let path = self.path(process, args[0])?;
                if !self.directories.contains(&path) {
                    return Ok((0, 3));
                }
                self.cwd = path;
                Ok((1, 0))
            }
            "GetCurrentDirectoryA" => {
                let value = paths::absolute(&self.cwd);
                if args[0] <= value.len() as u32 {
                    return Ok((value.len() as u32 + 1, 0));
                }
                Ok((output_string(process, args[1], args[0], &value)?, 0))
            }
            "GetFullPathNameA" => {
                let value = paths::absolute(&self.path(process, args[0])?);
                if args[1] <= value.len() as u32 {
                    return Ok((value.len() as u32 + 1, 0));
                }
                let length = output_string(process, args[2], args[1], &value)?;
                let offset = value.rfind('\\').map_or(0, |index| index + 1);
                write_u32(
                    process,
                    args[3],
                    args[2]
                        .checked_add(offset as u32)
                        .ok_or(Error::InvalidRange)?,
                )?;
                Ok((length, 0))
            }
            _ => Err(Error::Host(format!("unknown file API {name}"))),
        }
    }
    fn open<M: GuestMemory>(&mut self, process: &Process<M>, args: &[u32]) -> Result<(u32, u32)> {
        let spelling = self.cased_path(process, args[0])?;
        let path = spelling.to_lowercase();
        if path.is_empty() || path.contains(['*', '?']) {
            return Ok((u32::MAX, 123));
        }
        if self.directories.contains(&path) {
            return Ok((u32::MAX, 5));
        }
        if !self.directories.contains(paths::parent(&path)) {
            return Ok((u32::MAX, 3));
        }
        let existing = self.entries.get(&path).cloned();
        let writable = args[1] & 0x50000000 != 0;
        let readable = args[1] & 0x90000000 != 0;
        let creation = args[4];
        let error = match creation {
            1 if existing.is_some() => 80,
            3 | 5 if existing.is_none() => 2,
            2 | 5 if !writable => 5,
            1..=5 => 0,
            _ => 87,
        };
        if error != 0 {
            return Ok((u32::MAX, error));
        }
        if args[5] & 0x40000000 != 0 {
            return Ok((u32::MAX, 87));
        } // async I/O needs a scheduler contract
        let existed = existing.is_some();
        let node = existing.unwrap_or_else(|| {
            Rc::new(RefCell::new(Node {
                original: Arc::from([]),
                source: None,
                overlay: None,
            }))
        });
        if creation == 2 || creation == 5 {
            node.borrow_mut().overlay = Some(Vec::new());
        }
        let handle = self.handle_id()?;
        if !existed || creation == 2 || creation == 5 {
            self.record(
                journal::Change::Resize {
                    path: spelling.clone(),
                    size: 0,
                },
                &[],
            )?;
        }
        self.remember_name(&spelling);
        self.entries.insert(path.clone(), node.clone());
        self.handles.insert(
            handle,
            Handle {
                path,
                node,
                position: 0,
                readable,
                writable,
            },
        );
        Ok((
            handle,
            if existed && matches!(creation, 2 | 4) {
                183
            } else {
                0
            },
        ))
    }

    fn copy_or_move<M: GuestMemory>(
        &mut self,
        process: &Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        let path = self.path(process, args[0])?;
        let spelling = self.cased_path(process, args[1])?;
        let to = spelling.to_lowercase();
        let moving = name == "MoveFileA";
        let Some(source) = self.entries.get(&path) else {
            return Ok((0, 2));
        };
        if (moving || args[2] != 0) && self.entries.contains_key(&to) {
            return Ok((0, if moving { 183 } else { 80 }));
        }
        let bytes = source.borrow().bytes()?.into_owned();
        let change = if moving {
            journal::Change::Move {
                path: path.clone(),
                to: spelling.clone(),
            }
        } else {
            journal::Change::Copy {
                path: path.clone(),
                to: spelling.clone(),
            }
        };
        self.record(change, &[])?;
        self.add_parents(&spelling);
        self.remember_name(&spelling);
        self.entries.insert(
            to.clone(),
            Rc::new(RefCell::new(Node {
                original: Arc::from(bytes),
                source: None,
                overlay: None,
            })),
        );
        if moving {
            self.entries.remove(&path);
            self.names.remove(&path);
            for handle in self
                .handles
                .values_mut()
                .filter(|handle| handle.path == path)
            {
                handle.path = to.clone();
            }
        }
        Ok((1, 0))
    }
}
