//! The original `sims-web:registry-v2` typed JSON format, transferred once per mutation.
use super::{joined, Registry, Value};
use crate::{Error, Result};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

const ROOT_NAMES: [(&str, &str); 6] = [
    ("hkcr", "hkey_classes_root"),
    ("hkcu", "hkey_current_user"),
    ("hklm", "hkey_local_machine"),
    ("hku", "hkey_users"),
    ("hkpd", "hkey_performance_data"),
    ("hkcc", "hkey_current_config"),
];

#[derive(Serialize, Deserialize)]
struct StoredValue {
    t: u32,
    d: String,
}
type Snapshot = BTreeMap<String, BTreeMap<String, StoredValue>>;

fn path_name(path: &str, importing: bool) -> String {
    let normalized = joined("", path).trim_start_matches('\\').to_string();
    let (root, suffix) = normalized.split_once('\\').unwrap_or((&normalized, ""));
    let root = ROOT_NAMES
        .iter()
        .find_map(|(short, long)| {
            if importing && root == *long {
                Some(*short)
            } else if !importing && root == *short {
                Some(*long)
            } else {
                None
            }
        })
        .unwrap_or(root);
    joined(root, suffix)
}

impl Registry {
    /// Decode the entire snapshot before changing state. Invalid storage is not a partial import.
    pub fn import_snapshot(&mut self, json: &str) -> Result<()> {
        let snapshot: Snapshot = serde_json::from_str(json)
            .map_err(|error| Error::Host(format!("invalid registry snapshot: {error}")))?;
        let mut records = Vec::new();
        for (path, values) in snapshot {
            for (name, value) in values {
                let bytes = STANDARD
                    .decode(value.d)
                    .map_err(|error| Error::Host(format!("invalid registry value: {error}")))?;
                records.push((
                    path_name(&path, true),
                    name.to_lowercase(),
                    Value {
                        kind: value.t,
                        bytes,
                    },
                ));
            }
        }
        for (path, name, value) in records {
            self.create_path(&path)
                .borrow_mut()
                .values
                .insert(name, value);
        }
        Ok(())
    }

    pub fn take_changes(&mut self) -> Result<Option<String>> {
        if !self.dirty {
            return Ok(None);
        }
        let json = self.snapshot()?;
        self.dirty = false;
        Ok(Some(json))
    }

    fn snapshot(&self) -> Result<String> {
        let snapshot: Snapshot = self
            .keys
            .iter()
            .filter_map(|(path, key)| {
                let key = key.borrow();
                if key.deleted || key.values.is_empty() {
                    return None;
                }
                let values = key
                    .values
                    .iter()
                    .map(|(name, value)| {
                        (
                            name.clone(),
                            StoredValue {
                                t: value.kind,
                                d: STANDARD.encode(&value.bytes),
                            },
                        )
                    })
                    .collect();
                Some((path_name(path, false), values))
            })
            .collect();
        serde_json::to_string(&snapshot).map_err(|error| Error::Host(error.to_string()))
    }
}

#[cfg(test)]
#[path = "../../tests/unit/registry/persistence.rs"]
mod tests;
