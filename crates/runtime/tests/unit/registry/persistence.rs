use super::*;
#[test]
fn original_typed_storage_round_trips_without_changing_root_names() {
    let mut registry = Registry::default();
    let original = r#"{"hkey_local_machine\\software\\maxis":{"language":{"t":4,"d":"CQAAAA=="},"path":{"t":1,"d":"QzpcU2ltcwA="}}}"#;
    registry.import_snapshot(original).unwrap();
    let key = registry.keys["hklm\\software\\maxis"].borrow();
    assert_eq!(key.values["language"].bytes, [9, 0, 0, 0]);
    assert_eq!(key.values["path"].bytes, b"C:\\Sims\0");
    drop(key);
    assert_eq!(registry.snapshot().unwrap(), original);
    assert_eq!(registry.take_changes().unwrap(), None);
}

#[test]
fn malformed_data_does_not_partially_import_and_paths_are_normalized() {
    let mut registry = Registry::default();
    registry
        .import_snapshot(r#"{"HKEY_CURRENT_USER/Software/Test/":{"VALUE":{"t":3,"d":"AP8="}}}"#)
        .unwrap();
    let before = registry.snapshot().unwrap();
    assert!(before.contains("hkey_current_user\\\\software\\\\test"));
    assert!(registry
        .import_snapshot(
            r#"{"hkey_local_machine\\new":{"a":{"t":3,"d":"AA=="},"z":{"t":3,"d":"!"}}}"#
        )
        .is_err());
    assert_eq!(registry.snapshot().unwrap(), before);
    registry.dirty = true;
    assert_eq!(registry.take_changes().unwrap(), Some(before));
    assert_eq!(registry.take_changes().unwrap(), None);
}
