use super::complete_profile_json;
use serde_json::{json, Value};

#[test]
fn complete_profile_contains_only_compatibility_metadata() {
    let profile: Value = serde_json::from_str(complete_profile_json()).unwrap();
    let keys: Vec<_> = profile
        .as_object()
        .unwrap()
        .keys()
        .map(String::as_str)
        .collect();
    assert_eq!(
        keys,
        [
            "initialRegistry",
            "modules",
            "requiredFiles",
            "roots",
            "sha256"
        ]
    );
    assert_eq!(
        profile["sha256"],
        "6e598c22512a24659cbd0e533306abf3129733d67b1d120916b1b6b1581e29f2"
    );
    assert_eq!(
        profile["modules"],
        json!([
            {"name": "gimex.dll", "base": 0x10000000u32},
            {"name": "ijl10.dll", "base": 0x10100000u32}
        ])
    );
    let roots: Vec<_> = profile["roots"]
        .as_array()
        .unwrap()
        .iter()
        .map(|root| u32::try_from(root.as_u64().unwrap()).unwrap())
        .collect();
    assert!(roots.windows(2).all(|pair| pair[0] < pair[1]));
    for root in [0x498830, 0x499b54, 0x5b8dba, 0x665290] {
        assert!(roots.binary_search(&root).is_ok(), "missing root {root:#x}");
    }
    assert_eq!(profile["requiredFiles"].as_array().unwrap().len(), 15);
    assert!(profile["requiredFiles"]
        .as_array()
        .unwrap()
        .iter()
        .all(|path| {
            let path = path.as_str().unwrap();
            !path.starts_with('/') && !path.contains("..") && !path.contains(':')
        }));
    assert_eq!(profile["initialRegistry"].as_object().unwrap().len(), 1);
    let registry = &profile["initialRegistry"]["hkey_local_machine\\software\\maxis\\the sims"];
    assert_eq!(registry.as_object().unwrap().len(), 3);
    assert_eq!(
        registry["SIMS_GAME_EDITION"],
        json!({"t": 1, "d": "MjU1AA=="})
    );
}
