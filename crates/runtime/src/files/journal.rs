//! SMS1 save frames from the original save-journal.ts. The host only reads/writes OPFS bytes.
use super::*;
use serde::{Deserialize, Serialize};
const MAGIC: u32 = 0x31534d53;
const MAX: usize = 0x40000000;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub(super) enum Change {
    Resize {
        path: String,
        size: usize,
    },
    Write {
        path: String,
        offset: usize,
        size: usize,
    },
    Replace {
        path: String,
    },
    Delete {
        path: String,
    },
    Move {
        path: String,
        to: String,
    },
    Copy {
        path: String,
        to: String,
    },
}
impl Change {
    fn path(&self) -> &str {
        match self {
            Self::Resize { path, .. }
            | Self::Write { path, .. }
            | Self::Replace { path }
            | Self::Delete { path }
            | Self::Move { path, .. }
            | Self::Copy { path, .. } => path,
        }
    }
    fn validate(&self, length: usize) -> Result<()> {
        let path_ok = |path: &str| !path.is_empty() && path.len() < 32768;
        let valid = path_ok(self.path())
            && match self {
                Self::Resize { size, .. } => *size <= MAX,
                Self::Write { size, offset, .. } => {
                    *size <= MAX && *offset <= *size && length <= size - offset
                }
                Self::Move { to, .. } | Self::Copy { to, .. } => path_ok(to),
                _ => true,
            };
        if valid {
            Ok(())
        } else {
            Err(Error::Host("invalid save journal operation".into()))
        }
    }
}

impl Files {
    pub fn take_file_changes(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.journal)
    }

    pub(super) fn record(&mut self, change: Change, data: &[u8]) -> Result<()> {
        change.validate(data.len())?;
        let meta = serde_json::to_vec(&change).map_err(|error| Error::Host(error.to_string()))?;
        let length = meta
            .len()
            .checked_add(data.len())
            .filter(|n| *n <= MAX)
            .ok_or(Error::OutOfMemory)?;
        self.journal
            .try_reserve(16 + length)
            .map_err(|_| Error::OutOfMemory)?;
        let mut crc = crc32fast::Hasher::new();
        crc.update(&meta);
        crc.update(data);
        for word in [MAGIC, length as u32, meta.len() as u32, crc.finalize()] {
            self.journal.extend_from_slice(&word.to_le_bytes());
        }
        self.journal.extend_from_slice(&meta);
        self.journal.extend_from_slice(data);
        Ok(())
    }

    /// Replay complete, checked frames. The returned byte offset excludes a torn final frame.
    /// The caller discards the process on corruption and never truncates a corrupt journal.
    pub fn restore_journal(&mut self, bytes: &[u8]) -> Result<usize> {
        let mut end = 0;
        while bytes.len() - end >= 16 {
            let header = &bytes[end..end + 16];
            let word = |i| u32::from_le_bytes(header[i..i + 4].try_into().unwrap());
            let length = word(4) as usize;
            let meta_length = word(8) as usize;
            if word(0) != MAGIC || length > MAX || meta_length == 0 || meta_length > length {
                return Err(Error::Host(format!(
                    "invalid save journal frame at {end}; saved data preserved"
                )));
            }
            if length > bytes.len() - end - 16 {
                break;
            }
            let payload = &bytes[end + 16..end + 16 + length];
            if crc32fast::hash(payload) != word(12) {
                return Err(Error::Host(format!(
                    "save journal checksum mismatch at {end}; saved data preserved"
                )));
            }
            let change: Change = serde_json::from_slice(&payload[..meta_length])
                .map_err(|error| Error::Host(format!("invalid save metadata: {error}")))?;
            self.replay(change, &payload[meta_length..])?;
            end += 16 + length;
        }
        Ok(end)
    }

    fn replay(&mut self, change: Change, data: &[u8]) -> Result<()> {
        change.validate(data.len())?;
        let spelling = paths::preserve_case("", change.path());
        let path = spelling.to_lowercase();
        let content = match &change {
            Change::Delete { .. } => {
                self.entries.remove(&path);
                self.names.remove(&path);
                return Ok(());
            }
            Change::Move { to, .. } | Change::Copy { to, .. } => {
                let bytes = self
                    .entries
                    .get(&path)
                    .ok_or_else(|| Error::Host(format!("missing saved source: {path}")))?
                    .borrow()
                    .bytes()?
                    .to_vec();
                self.replace_saved(&paths::preserve_case("", to), bytes);
                if matches!(change, Change::Move { .. }) {
                    self.entries.remove(&path);
                    self.names.remove(&path);
                }
                return Ok(());
            }
            Change::Replace { .. } => data.to_vec(),
            Change::Write { size, .. } | Change::Resize { size, .. } => {
                let mut bytes = Vec::new();
                bytes
                    .try_reserve_exact(*size)
                    .map_err(|_| Error::OutOfMemory)?;
                bytes.resize(*size, 0);
                if let Some(node) = self.entries.get(&path) {
                    let node = node.borrow();
                    let count = bytes.len().min(node.size() as usize);
                    if count != 0 {
                        bytes[..count].copy_from_slice(&node.read(0, count)?);
                    }
                }
                if let Change::Write { offset, .. } = change {
                    bytes[offset..offset + data.len()].copy_from_slice(data);
                }
                bytes
            }
        };
        self.replace_saved(&spelling, content);
        Ok(())
    }

    fn replace_saved(&mut self, path: &str, bytes: Vec<u8>) {
        self.add_parents(path);
        self.remember_name(path);
        self.entries.insert(
            path.to_lowercase(),
            Rc::new(RefCell::new(Node {
                original: Arc::from([]),
                source: None,
                overlay: Some(bytes),
            })),
        );
    }
}
