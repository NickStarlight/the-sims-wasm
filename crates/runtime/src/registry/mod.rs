//! Process-local registry roots, keys, handles, and typed value bytes. No installation data is invented.
mod keys;
mod persistence;
#[cfg(test)]
#[path = "../../tests/unit/registry/mod.rs"]
mod tests;
mod values;
use crate::{Error, GuestMemory, Process, Result};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};
const INVALID_HANDLE: u32 = 6;
const KEY_DELETED: u32 = 1018;
const ALL_ACCESS: u32 = 0xf003f;
const ROOTS: [(u32, &str); 6] = [
    (0x80000000, "hkcr"),
    (0x80000001, "hkcu"),
    (0x80000002, "hklm"),
    (0x80000003, "hku"),
    (0x80000004, "hkpd"),
    (0x80000005, "hkcc"),
];
struct Value {
    kind: u32,
    bytes: Vec<u8>,
}
#[derive(Default)]
struct Key {
    values: BTreeMap<String, Value>,
    deleted: bool,
}
#[derive(Clone)]
struct Handle {
    path: String,
    key: Rc<RefCell<Key>>,
    access: u32,
}
pub(crate) struct Registry {
    keys: BTreeMap<String, Rc<RefCell<Key>>>,
    handles: BTreeMap<u32, Handle>,
    next_handle: u32,
    dirty: bool,
}
impl Default for Registry {
    fn default() -> Self {
        Self {
            keys: ROOTS
                .iter()
                .map(|(_, name)| (name.to_string(), Rc::new(RefCell::new(Key::default()))))
                .collect(),
            handles: BTreeMap::new(),
            next_handle: 0x50000000,
            dirty: false,
        }
    }
}
impl Registry {
    pub fn call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        name: &str,
        args: &[u32],
    ) -> Result<u32> {
        let result = match name {
            "RegOpenKeyExA" | "RegOpenKeyA" | "RegCreateKeyExA" | "RegCreateKeyA"
            | "RegCloseKey" | "RegDeleteKeyA" => self.key_call(process, name, args),
            "RegQueryValueExA" | "RegSetValueExA" | "RegDeleteValueA" | "RegQueryValueA"
            | "RegSetValueA" => self.value_call(process, name, args),
            _ => Err(Error::Host(format!("unknown registry API {name}"))),
        }?;
        if result == 0
            && matches!(
                name,
                "RegCreateKeyExA"
                    | "RegCreateKeyA"
                    | "RegDeleteKeyA"
                    | "RegSetValueExA"
                    | "RegSetValueA"
                    | "RegDeleteValueA"
            )
        {
            self.dirty = true;
        }
        Ok(result)
    }
    fn handle(&self, id: u32) -> std::result::Result<Handle, u32> {
        let handle = if let Some((_, root)) = ROOTS.iter().find(|(value, _)| *value == id) {
            Handle {
                path: root.to_string(),
                key: self.keys[*root].clone(),
                access: ALL_ACCESS,
            }
        } else {
            self.handles.get(&id).cloned().ok_or(INVALID_HANDLE)?
        };
        if handle.key.borrow().deleted {
            return Err(KEY_DELETED);
        }
        Ok(handle)
    }
    fn allocate_handle(&mut self, handle: Handle) -> Result<u32> {
        let id = self.next_handle;
        self.next_handle = self
            .next_handle
            .checked_add(1)
            .filter(|id| *id < 0x60000000)
            .ok_or(Error::OutOfMemory)?;
        self.handles.insert(id, handle);
        Ok(id)
    }
    fn create_path(&mut self, path: &str) -> Rc<RefCell<Key>> {
        let mut current = String::new();
        for part in path.split('\\') {
            if !current.is_empty() {
                current.push('\\');
            }
            current.push_str(part);
            self.keys
                .entry(current.clone())
                .or_insert_with(|| Rc::new(RefCell::new(Key::default())));
        }
        self.keys[path].clone()
    }
}
fn joined(base: &str, sub: &str) -> String {
    let sub = sub
        .split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("\\")
        .to_lowercase();
    if sub.is_empty() {
        base.into()
    } else {
        format!("{base}\\{sub}")
    }
}
fn string<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<String> {
    if address == 0 {
        return Ok(String::new());
    }
    Ok(process
        .read_c_string(address, 32768)?
        .into_iter()
        .map(char::from)
        .collect())
}
fn read_u32<M: GuestMemory>(process: &Process<M>, address: u32) -> Result<u32> {
    let mut bytes = [0; 4];
    process.read(address, &mut bytes)?;
    Ok(u32::from_le_bytes(bytes))
}
fn write_u32<M: GuestMemory>(process: &mut Process<M>, address: u32, value: u32) -> Result<()> {
    if address != 0 {
        process.write(address, &value.to_le_bytes())?;
    }
    Ok(())
}
