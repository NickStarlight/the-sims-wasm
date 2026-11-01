//! Deserialize the loader's coarse image/export plan without a compiler dependency.
use super::*;
use crate::{Error, Result};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Configuration {
    #[serde(default = "default_executable")]
    executable_path: String,
    images: Vec<Image>,
    #[serde(default)]
    native_dlls: Vec<String>,
    #[serde(default)]
    native_bindings: Vec<Binding>,
}
fn default_executable() -> String {
    "C:\\Sims\\Sims.exe".into()
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Image {
    name: Option<String>,
    path: Option<String>,
    base: u32,
    size_of_image: u32,
    is_dll: bool,
    #[serde(default)]
    exports: Vec<Export>,
}
#[derive(Deserialize)]
struct Export {
    ordinal: u32,
    name: Option<String>,
    target: ExportTarget,
}
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
enum ExportTarget {
    Address { address: u32 },
    ForwardByName { dll: String, name: String },
    ForwardByOrdinal { dll: String, ordinal: u32 },
}
#[derive(Deserialize)]
struct Binding {
    dll: String,
    name: String,
    address: u32,
}
fn invalid(message: impl Into<String>) -> Error {
    Error::Host(message.into())
}

impl ModuleCatalog {
    pub(crate) fn from_json(json: &str) -> Result<Self> {
        let config: Configuration = serde_json::from_str(json)
            .map_err(|error| invalid(format!("invalid module configuration: {error}")))?;
        let mut catalog = Self::default();
        let mut ranges = Vec::new();
        for image in config.images {
            let end = u64::from(image.base) + u64::from(image.size_of_image);
            if image.base == 0
                || image.size_of_image == 0
                || end > 1u64 << 32
                || ranges
                    .iter()
                    .any(|&(base, limit)| u64::from(image.base) < limit && base < end)
            {
                return Err(invalid("invalid or overlapping module image"));
            }
            ranges.push((u64::from(image.base), end));
            let path = image
                .path
                .or_else(|| {
                    if image.is_dll {
                        image.name.map(|name| format!("C:\\Sims\\{name}"))
                    } else {
                        Some(config.executable_path.clone())
                    }
                })
                .ok_or_else(|| invalid("loaded DLL requires a name or path"))?;
            if !image.is_dll && catalog.executable.replace(image.base).is_some() {
                return Err(invalid("multiple executable modules"));
            }
            let mut exports = BTreeMap::new();
            for export in image.exports {
                let target = match export.target {
                    ExportTarget::Address { address } => {
                        if address < image.base || u64::from(address) >= end {
                            return Err(invalid("export address outside module image"));
                        }
                        Target::Address(address)
                    }
                    ExportTarget::ForwardByName { dll, name } => Target::Forward {
                        dll,
                        symbol: Symbol::Name(name),
                    },
                    ExportTarget::ForwardByOrdinal { dll, ordinal } => Target::Forward {
                        dll,
                        symbol: Symbol::Ordinal(ordinal),
                    },
                };
                insert_export(
                    &mut exports,
                    Symbol::Ordinal(export.ordinal),
                    target.clone(),
                )?;
                if let Some(name) = export.name {
                    insert_export(&mut exports, Symbol::Name(name), target)?;
                }
            }
            catalog.insert(image.base, path, exports)?;
        }
        if config
            .native_dlls
            .iter()
            .chain(config.native_bindings.iter().map(|binding| &binding.dll))
            .any(|name| name.is_empty() || name.contains('\0'))
        {
            return Err(invalid("empty or invalid native DLL name"));
        }
        let native: BTreeSet<_> = config
            .native_dlls
            .iter()
            .chain(config.native_bindings.iter().map(|b| &b.dll))
            .map(|name| canonical(name))
            .collect();
        for (index, name) in native.into_iter().enumerate() {
            if name.contains(['\\', ':']) || name.is_empty() || catalog.names.contains_key(&name) {
                return Err(invalid(
                    "native DLL name conflicts with an image or contains a path",
                ));
            }
            let handle = 0x7f000000u32
                .checked_add(
                    u32::try_from(index)
                        .map_err(|_| invalid("too many native DLLs"))?
                        .checked_mul(0x10000)
                        .ok_or_else(|| invalid("too many native DLLs"))?,
                )
                .ok_or_else(|| invalid("too many native DLLs"))?;
            if ranges
                .iter()
                .any(|&(base, end)| u64::from(handle) >= base && u64::from(handle) < end)
            {
                return Err(invalid("native module handle collides with guest image"));
            }
            catalog.insert(
                handle,
                format!("C:\\Windows\\System32\\{name}"),
                BTreeMap::new(),
            )?;
        }
        for binding in config.native_bindings {
            if binding.address == 0 || binding.name.is_empty() {
                return Err(invalid("invalid native procedure binding"));
            }
            let handle = catalog
                .handle(Some(&binding.dll))
                .ok_or_else(|| invalid("missing native DLL"))?;
            insert_export(
                &mut catalog
                    .modules
                    .get_mut(&handle)
                    .expect("installed native DLL")
                    .exports,
                Symbol::Name(binding.name),
                Target::Address(binding.address),
            )?;
        }
        Ok(catalog)
    }

    fn insert(
        &mut self,
        handle: u32,
        path: String,
        exports: BTreeMap<Symbol, Target>,
    ) -> Result<()> {
        if path.is_empty() || path.contains('\0') || path.encode_utf16().count() >= 32768 {
            return Err(invalid("invalid module path"));
        }
        let key = canonical(&path);
        if self.modules.contains_key(&handle) || self.paths.contains_key(&key) {
            return Err(invalid("duplicate module handle or path"));
        }
        let name = key.rsplit('\\').next().unwrap_or(&key).to_owned();
        self.names.entry(name).or_insert(handle);
        self.paths.insert(key, handle);
        self.modules.insert(
            handle,
            Module {
                path,
                exports,
                dynamic_references: 0,
            },
        );
        Ok(())
    }
}

fn insert_export(
    exports: &mut BTreeMap<Symbol, Target>,
    symbol: Symbol,
    target: Target,
) -> Result<()> {
    if exports
        .get(&symbol)
        .is_some_and(|previous| previous != &target)
    {
        return Err(invalid("conflicting module exports"));
    }
    exports.insert(symbol, target);
    Ok(())
}
