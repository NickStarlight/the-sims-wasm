//! Lookup of explicitly installed guest images and native compatibility libraries.
mod config;
#[cfg(test)]
#[path = "../tests/unit/modules.rs"]
pub(crate) mod tests;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Symbol {
    Name(String),
    Ordinal(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Target {
    Address(u32),
    Forward { dll: String, symbol: Symbol },
}

struct Module {
    path: String,
    exports: BTreeMap<Symbol, Target>,
    /// References from LoadLibrary only; the loader keeps an independent lifetime.
    dynamic_references: u32,
}

#[derive(Default)]
pub(crate) struct ModuleCatalog {
    executable: Option<u32>,
    modules: BTreeMap<u32, Module>,
    names: BTreeMap<String, u32>,
    paths: BTreeMap<String, u32>,
}

impl ModuleCatalog {
    pub(crate) fn handle(&self, name: Option<&str>) -> Option<u32> {
        let Some(name) = name else {
            return self.executable;
        };
        let key = canonical(name);
        if key.contains('\\') || key.contains(':') {
            self.paths.get(&key).copied()
        } else {
            self.names.get(&key).copied()
        }
    }

    pub(crate) fn filename(&self, handle: u32) -> Option<&str> {
        let handle = if handle == 0 {
            self.executable?
        } else {
            handle
        };
        self.modules.get(&handle).map(|module| module.path.as_str())
    }

    pub(crate) fn load(&mut self, name: &str) -> Option<u32> {
        let handle = self.handle(Some(name))?;
        let module = self.modules.get_mut(&handle)?;
        module.dynamic_references = module.dynamic_references.checked_add(1)?;
        Some(handle)
    }

    pub(crate) fn free(&mut self, handle: u32) -> bool {
        let Some(module) = self.modules.get_mut(&handle) else {
            return false;
        };
        let Some(references) = module.dynamic_references.checked_sub(1) else {
            return false;
        };
        module.dynamic_references = references;
        true
    }

    pub(crate) fn procedure(&self, handle: u32, symbol: Symbol) -> std::result::Result<u32, u32> {
        if !self.modules.contains_key(&handle) {
            return Err(6);
        }
        let mut current = (handle, symbol);
        let mut visited = BTreeSet::new();
        while visited.len() < 32 && visited.insert(current.clone()) {
            let module = self.modules.get(&current.0).ok_or(127u32)?;
            match module.exports.get(&current.1).ok_or(127u32)? {
                Target::Address(address) => return Ok(*address),
                Target::Forward { dll, symbol } => {
                    current = (self.handle(Some(dll)).ok_or(127u32)?, symbol.clone());
                }
            }
        }
        Err(127)
    }
}

/// Match Windows module names without conflating explicit paths and basenames.
fn canonical(name: &str) -> String {
    let mut name = name.replace('/', "\\").to_ascii_lowercase();
    if name.ends_with('.') {
        name.pop();
    } else if !name.rsplit('\\').next().unwrap_or("").contains('.') {
        name.push_str(".dll");
    }
    name
}
