//! Snapshot directory enumeration using WIN32_FIND_DATAA records.
use super::*;
impl Files {
    pub(super) fn search<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<(u32, u32)> {
        match name {
            "FindFirstFileA" => {
                let pattern = self.path(process, args[0])?;
                let parent = paths::parent(&pattern);
                if !self.directories.contains(parent) {
                    return Ok((u32::MAX, 3));
                }
                let mask = pattern.rsplit('/').next().unwrap_or("");
                let mut entries = Vec::new();
                for (path, node) in &self.entries {
                    let name = path.rsplit('/').next().unwrap_or("");
                    if paths::parent(path) == parent && paths::wildcard(mask, name) {
                        entries.push(Entry {
                            name: self.names.get(path).cloned().unwrap_or_else(|| name.into()),
                            size: node.borrow().size(),
                            directory: false,
                        });
                    }
                }
                for path in &self.directories {
                    let name = path.rsplit('/').next().unwrap_or("");
                    if !path.is_empty()
                        && paths::parent(path) == parent
                        && paths::wildcard(mask, name)
                    {
                        entries.push(Entry {
                            name: self.names.get(path).cloned().unwrap_or_else(|| name.into()),
                            size: 0,
                            directory: true,
                        });
                    }
                }
                entries.sort_by_key(|entry| entry.name.to_lowercase());
                // Preserve the original scanner contract: the game consumes the first
                // two wildcard results as dot directories before visiting real files.
                if !entries.is_empty() && paths::wildcard(mask, ".") && paths::wildcard(mask, "..")
                {
                    let dots = [".", ".."].map(|name| Entry {
                        name: name.into(),
                        size: 0,
                        directory: true,
                    });
                    entries.splice(0..0, dots);
                }
                let Some(first) = entries.first() else {
                    return Ok((u32::MAX, 2));
                };
                write_entry(process, args[1], first)?;
                let id = self.handle_id()?;
                self.searches.insert(
                    id,
                    Search {
                        entries,
                        position: 1,
                    },
                );
                Ok((id, 0))
            }
            "FindNextFileA" => {
                let Some(search) = self.searches.get_mut(&args[0]) else {
                    return Ok((0, 6));
                };
                let Some(entry) = search.entries.get(search.position) else {
                    return Ok((0, 18));
                };
                write_entry(process, args[1], entry)?;
                search.position += 1;
                Ok((1, 0))
            }
            "FindClose" => Ok(if self.searches.remove(&args[0]).is_some() {
                (1, 0)
            } else {
                (0, 6)
            }),
            _ => Err(Error::Host(format!("unknown directory API {name}"))),
        }
    }
}
fn write_entry<M: GuestMemory>(
    process: &mut Process<M>,
    address: u32,
    entry: &Entry,
) -> Result<()> {
    let mut record = [0; 320];
    record[..4].copy_from_slice(&(if entry.directory { 0x10u32 } else { 0x20 }).to_le_bytes());
    record[28..32].copy_from_slice(&((entry.size >> 32) as u32).to_le_bytes());
    record[32..36].copy_from_slice(&(entry.size as u32).to_le_bytes());
    let name = entry.name.as_bytes();
    let length = name.len().min(259);
    record[44..44 + length].copy_from_slice(&name[..length]);
    process.write(address, &record)
}
