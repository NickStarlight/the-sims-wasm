//! Process-owned preloaded files, copy-on-write overlays, handles, and directory searches.
mod calls;
mod journal;
#[cfg(test)]
#[path = "../../tests/unit/files/journal.rs"]
mod journal_tests;
mod paths;
mod profile;
mod search;
#[cfg(test)]
#[path = "../../tests/unit/files/mod.rs"]
mod tests;
mod transfer;
use crate::{Error, GuestMemory, Process, Result};
use std::{
    borrow::Cow,
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    sync::Arc,
};

struct Node {
    original: Arc<[u8]>,
    source: Option<Rc<FileSource>>,
    overlay: Option<Vec<u8>>,
}
pub(crate) type FileReader = dyn Fn(u64, usize) -> Result<Vec<u8>>;
struct FileSource {
    size: u64,
    read: Rc<FileReader>,
}
impl Node {
    fn size(&self) -> u64 {
        self.overlay.as_ref().map_or_else(
            || {
                self.source
                    .as_ref()
                    .map_or(self.original.len() as u64, |s| s.size)
            },
            |bytes| bytes.len() as u64,
        )
    }
    fn read(&self, offset: u64, length: usize) -> Result<Cow<'_, [u8]>> {
        let offset = offset.min(self.size());
        let length = length.min((self.size() - offset) as usize);
        if length == 0 {
            return Ok(Cow::Borrowed(&[]));
        }
        if let Some(bytes) = &self.overlay {
            return Ok(Cow::Borrowed(
                &bytes[offset as usize..offset as usize + length],
            ));
        }
        if let Some(source) = &self.source {
            let bytes = (source.read)(offset, length)?;
            if bytes.len() != length {
                return Err(Error::Host("short read from selected game file".into()));
            }
            return Ok(Cow::Owned(bytes));
        }
        Ok(Cow::Borrowed(
            &self.original[offset as usize..offset as usize + length],
        ))
    }
    fn bytes(&self) -> Result<Cow<'_, [u8]>> {
        self.read(0, self.size() as usize)
    }
    fn writable(&mut self) -> Result<&mut Vec<u8>> {
        if self.overlay.is_none() {
            self.overlay = Some(self.bytes()?.into_owned());
        }
        Ok(self.overlay.as_mut().unwrap())
    }
}
struct Handle {
    path: String,
    node: Rc<RefCell<Node>>,
    position: u64,
    readable: bool,
    writable: bool,
}
#[derive(Clone)]
struct Entry {
    name: String,
    size: u64,
    directory: bool,
}
struct Search {
    entries: Vec<Entry>,
    position: usize,
}

pub(crate) struct Files {
    entries: BTreeMap<String, Rc<RefCell<Node>>>,
    directories: BTreeSet<String>,
    // Lookup keys remain folded; enumeration returns the name supplied on creation.
    names: BTreeMap<String, String>,
    handles: BTreeMap<u32, Handle>,
    searches: BTreeMap<u32, Search>,
    next_handle: u32,
    cwd: String,
    journal: Vec<u8>,
}
impl Default for Files {
    fn default() -> Self {
        Self {
            entries: BTreeMap::new(),
            directories: BTreeSet::from([String::new()]),
            names: BTreeMap::new(),
            handles: BTreeMap::new(),
            searches: BTreeMap::new(),
            next_handle: 0x40000000,
            cwd: String::new(),
            journal: Vec::new(),
        }
    }
}
impl Files {
    pub(crate) fn file_bytes(&self, path: &str) -> Result<Option<Vec<u8>>> {
        self.entries
            .get(&paths::normalize(&self.cwd, path))
            .map(|node| node.borrow().bytes().map(Cow::into_owned))
            .transpose()
    }

    pub(crate) fn add_file_source(
        &mut self,
        path: &str,
        size: u64,
        read: Rc<FileReader>,
    ) -> Result<()> {
        if size > u64::from(u32::MAX) {
            return Err(Error::InvalidRange);
        }
        self.add_file(path, &[])?;
        let path = paths::normalize("", path);
        self.entries[&path].borrow_mut().source = Some(Rc::new(FileSource { size, read }));
        Ok(())
    }

    /// The supplied bytes are copied once into immutable process-owned backing.
    pub fn add_file(&mut self, path: &str, bytes: &[u8]) -> Result<()> {
        let spelling = paths::preserve_case("", path);
        let path = spelling.to_lowercase();
        if path.is_empty() || path.contains(['*', '?']) {
            return Err(Error::Host("invalid preload path".into()));
        }
        if self.entries.contains_key(&path) {
            return Err(Error::Host(format!("duplicate preload path {path}")));
        }
        self.add_parents(&spelling);
        self.remember_name(&spelling);
        self.entries.insert(
            path,
            Rc::new(RefCell::new(Node {
                original: Arc::from(bytes),
                source: None,
                overlay: None,
            })),
        );
        Ok(())
    }
    fn add_parents(&mut self, path: &str) {
        let mut parent = path;
        while let Some((directory, _)) = parent.rsplit_once('/') {
            self.directories.insert(directory.to_lowercase());
            self.remember_name(directory);
            parent = directory;
        }
    }
    fn remember_name(&mut self, path: &str) {
        self.names
            .entry(path.to_lowercase())
            .or_insert_with(|| path.rsplit('/').next().unwrap_or("").to_owned());
    }
    pub fn owns_handle(&self, handle: u32) -> bool {
        self.handles.contains_key(&handle)
    }
    fn handle_id(&mut self) -> Result<u32> {
        let handle = self.next_handle;
        self.next_handle = self.next_handle.checked_add(1).ok_or(Error::OutOfMemory)?;
        if handle >= 0x80000000 {
            return Err(Error::OutOfMemory);
        }
        Ok(handle)
    }
    fn path<M: GuestMemory>(&self, process: &Process<M>, pointer: u32) -> Result<String> {
        Ok(paths::normalize(&self.cwd, &read_string(process, pointer)?))
    }
    fn cased_path<M: GuestMemory>(&self, process: &Process<M>, pointer: u32) -> Result<String> {
        Ok(paths::preserve_case(
            &self.cwd,
            &read_string(process, pointer)?,
        ))
    }
}

fn read_string<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<String> {
    if address == 0 {
        return Ok(String::new());
    }
    Ok(process
        .read_c_string(address, 32768)?
        .into_iter()
        .map(char::from)
        .collect())
}
fn write_u32<M: GuestMemory>(process: &mut Process<M>, address: u32, value: u32) -> Result<()> {
    if address != 0 {
        process.write(address, &value.to_le_bytes())?;
    }
    Ok(())
}
fn read_u32<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<u32> {
    let mut bytes = [0; 4];
    process.read(address, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn output_string<M: GuestMemory>(
    process: &mut Process<M>,
    address: u32,
    capacity: u32,
    value: &str,
) -> Result<u32> {
    if capacity == 0 {
        return Ok(0);
    }
    let mut bytes: Vec<_> = value.bytes().take((capacity - 1) as usize).collect();
    let length = bytes.len() as u32;
    bytes.push(0);
    process.write(address, &bytes)?;
    Ok(length)
}
