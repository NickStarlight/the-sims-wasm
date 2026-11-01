use super::*;

pub(crate) fn configuration() -> String {
    serde_json::json!({
        "executablePath": "C:\\Games\\Sims.exe",
        "images": [
            {"base": 4194304, "sizeOfImage": 8192, "isDll": false, "exports": []},
            {"name": "Gonzo.dll", "base": 268435456, "sizeOfImage": 8192, "isDll": true,
             "exports": [
                {"ordinal": 1, "name": "Local", "target": {"kind": "address", "address": 268439552}},
                {"ordinal": 2, "name": "Forward", "target": {"kind": "forwardByName", "dll": "KERNEL32", "name": "HeapAlloc"}},
                {"ordinal": 3, "name": "Alias", "target": {"kind": "forwardByOrdinal", "dll": "gonzo", "ordinal": 1}},
                {"ordinal": 4, "name": "Cycle", "target": {"kind": "forwardByName", "dll": "gonzo", "name": "Cycle"}}
             ]}
        ],
        "nativeDlls": ["KERNEL32.dll"],
        "nativeBindings": [{"dll": "kernel32", "name": "HeapAlloc", "address": 8388864}]
    }).to_string()
}

#[test]
fn lookup_preserves_paths_case_and_export_forwarding() {
    let catalog = ModuleCatalog::from_json(&configuration()).unwrap();
    assert_eq!(catalog.handle(None), Some(0x400000));
    assert_eq!(catalog.handle(Some("GONZO")), Some(0x10000000));
    assert_eq!(catalog.handle(Some("C:/Sims/Gonzo.dll")), Some(0x10000000));
    assert_eq!(catalog.handle(Some("C:/Other/Gonzo.dll")), None);
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Name("Local".into())),
        Ok(0x10001000)
    );
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Ordinal(1)),
        Ok(0x10001000)
    );
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Name("Forward".into())),
        Ok(0x800100)
    );
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Name("Alias".into())),
        Ok(0x10001000)
    );
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Name("local".into())),
        Err(127)
    );
    assert_eq!(
        catalog.procedure(0x10000000, Symbol::Name("Cycle".into())),
        Err(127)
    );
    assert_eq!(catalog.procedure(1, Symbol::Ordinal(1)), Err(6));
}

#[test]
fn only_explicit_libraries_exist_and_load_references_balance() {
    let mut catalog = ModuleCatalog::from_json(&configuration()).unwrap();
    let native = catalog.handle(Some("kernel32")).unwrap();
    assert_eq!(catalog.load("missing.dll"), None);
    assert_eq!(catalog.handle(Some("missing.dll")), None);
    assert!(!catalog.free(native));
    assert_eq!(catalog.load("KERNEL32.dll"), Some(native));
    assert_eq!(catalog.load("kernel32"), Some(native));
    assert!(catalog.free(native));
    assert!(catalog.free(native));
    assert!(!catalog.free(native));
    assert_eq!(catalog.handle(Some("kernel32")), Some(native)); // loader-owned backend remains installed.
}

#[test]
fn malformed_configuration_is_rejected() {
    let config: serde_json::Value = serde_json::from_str(&configuration()).unwrap();
    for (field, value) in [("base", 0), ("sizeOfImage", 0), ("base", 0x400000)] {
        let mut invalid = config.clone();
        invalid["images"][1][field] = value.into();
        assert!(ModuleCatalog::from_json(&invalid.to_string()).is_err());
    }
    let mut invalid = config.clone();
    invalid["images"][1]["exports"][0]["target"]["address"] = 1.into();
    assert!(ModuleCatalog::from_json(&invalid.to_string()).is_err());
    let mut invalid = config;
    invalid["nativeDlls"] = serde_json::json!(["Gonzo.dll"]);
    assert!(ModuleCatalog::from_json(&invalid.to_string()).is_err());
}
