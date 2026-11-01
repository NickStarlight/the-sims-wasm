use super::*;
use crate::SparseMemory;
const HKLM: u32 = 0x80000002;
fn fixture() -> (Registry, Process<SparseMemory>) {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x1000, &vec![0; 4096], true).unwrap();
    (Registry::default(), process)
}
fn text(process: &mut Process<SparseMemory>, address: u32, text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    process.write(address, &bytes).unwrap();
}
fn call(
    registry: &mut Registry,
    process: &mut Process<SparseMemory>,
    name: &str,
    args: &[u32],
) -> u32 {
    registry.call(process, name, args).unwrap()
}
fn create(registry: &mut Registry, process: &mut Process<SparseMemory>, path: &str) -> u32 {
    text(process, 0x1000, path);
    assert_eq!(
        call(
            registry,
            process,
            "RegCreateKeyExA",
            &[HKLM, 0x1000, 0, 0, 0, ALL_ACCESS, 0, 0x1100, 0x1104]
        ),
        0
    );
    read_u32(process, 0x1100).unwrap()
}
#[test]
fn create_dispositions_and_case_insensitive_open_are_correct() {
    let (mut registry, mut process) = fixture();
    let handle = create(&mut registry, &mut process, "Software\\Maxis\\The Sims");
    assert_eq!(read_u32(&process, 0x1104).unwrap(), 1);
    let second = create(&mut registry, &mut process, "SOFTWARE\\maxis\\the sims");
    assert_ne!(handle, second);
    assert_eq!(read_u32(&process, 0x1104).unwrap(), 2);
    text(&mut process, 0x1000, "software\\MAXIS");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyExA",
            &[HKLM, 0x1000, 0, 1, 0x1100]
        ),
        0
    );
}
#[test]
fn query_size_and_short_buffer_leave_data_untouched() {
    let (mut registry, mut process) = fixture();
    let handle = create(&mut registry, &mut process, "Software\\Test");
    text(&mut process, 0x1200, "Data");
    process.write(0x1300, &[1, 0, 2, 3, 0]).unwrap();
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegSetValueExA",
            &[handle, 0x1200, 0, 3, 0x1300, 5]
        ),
        0
    );
    text(&mut process, 0x1200, "DATA");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueExA",
            &[handle, 0x1200, 0, 0x1400, 0, 0x1404]
        ),
        0
    );
    assert_eq!(read_u32(&process, 0x1400).unwrap(), 3);
    assert_eq!(read_u32(&process, 0x1404).unwrap(), 5);
    process.write(0x1500, &[9; 5]).unwrap();
    write_u32(&mut process, 0x1404, 4).unwrap();
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueExA",
            &[handle, 0x1200, 0, 0x1400, 0x1500, 0x1404]
        ),
        234
    );
    let mut bytes = [0; 5];
    process.read(0x1500, &mut bytes).unwrap();
    assert_eq!(bytes, [9; 5]);
    assert_eq!(read_u32(&process, 0x1404).unwrap(), 5);
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueExA",
            &[handle, 0x1200, 0, 0, 0x1500, 0x1404]
        ),
        0
    );
    process.read(0x1500, &mut bytes).unwrap();
    assert_eq!(bytes, [1, 0, 2, 3, 0]);
}
#[test]
fn root_handles_survive_close_and_no_installation_values_are_seeded() {
    let (mut registry, mut process) = fixture();
    assert_eq!(call(&mut registry, &mut process, "RegCloseKey", &[HKLM]), 0);
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyExA",
            &[HKLM, 0, 0, ALL_ACCESS, 0x1100]
        ),
        0
    );
    assert_eq!(read_u32(&process, 0x1100).unwrap(), HKLM);
    text(&mut process, 0x1000, "Software\\Maxis\\The Sims");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyExA",
            &[HKLM, 0x1000, 0, 1, 0x1100]
        ),
        2
    );
    assert_eq!(
        call(&mut registry, &mut process, "RegCloseKey", &[123]),
        INVALID_HANDLE
    );
}
#[test]
fn deletion_is_nonrecursive_and_stale_handles_do_not_alias_recreated_keys() {
    let (mut registry, mut process) = fixture();
    let old = create(&mut registry, &mut process, "Parent\\Child");
    text(&mut process, 0x1000, "Parent");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegDeleteKeyA",
            &[HKLM, 0x1000]
        ),
        5
    );
    text(&mut process, 0x1000, "Parent\\Child");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegDeleteKeyA",
            &[HKLM, 0x1000]
        ),
        0
    );
    let new = create(&mut registry, &mut process, "Parent\\Child");
    assert_ne!(old, new);
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueExA",
            &[old, 0, 0, 0, 0, 0]
        ),
        KEY_DELETED
    );
    assert_eq!(call(&mut registry, &mut process, "RegCloseKey", &[old]), 0);
    assert_eq!(
        call(&mut registry, &mut process, "RegCloseKey", &[old]),
        INVALID_HANDLE
    );
}
#[test]
fn read_only_handles_and_invalid_guest_payloads_cannot_mutate_values() {
    let (mut registry, mut process) = fixture();
    let handle = create(&mut registry, &mut process, "Test");
    assert_eq!(
        registry.call(
            &mut process,
            "RegSetValueExA",
            &[handle, 0, 0, 3, 0x9000, 4]
        ),
        Err(Error::Unmapped)
    );
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueExA",
            &[handle, 0, 0, 0, 0, 0]
        ),
        2
    );
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyExA",
            &[HKLM, 0x1000, 0, 1, 0x1100]
        ),
        0
    );
    let read_only = read_u32(&process, 0x1100).unwrap();
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegSetValueExA",
            &[read_only, 0, 0, 3, 0, 0]
        ),
        5
    );
}
#[test]
fn legacy_default_string_values_and_delete_value_work() {
    let (mut registry, mut process) = fixture();
    text(&mut process, 0x1000, "Legacy");
    text(&mut process, 0x1200, "Game");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegSetValueA",
            &[HKLM, 0x1000, 1, 0x1200, 1]
        ),
        0
    );
    write_u32(&mut process, 0x1400, 32).unwrap();
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegQueryValueA",
            &[HKLM, 0x1000, 0x1500, 0x1400]
        ),
        0
    );
    assert_eq!(read_u32(&process, 0x1400).unwrap(), 5);
    assert_eq!(string(&process, 0x1500).unwrap(), "Game");
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyA",
            &[HKLM, 0x1000, 0x1100]
        ),
        0
    );
    let handle = read_u32(&process, 0x1100).unwrap();
    assert_eq!(
        call(&mut registry, &mut process, "RegDeleteValueA", &[handle, 0]),
        0
    );
    assert_eq!(
        call(&mut registry, &mut process, "RegDeleteValueA", &[handle, 0]),
        2
    );
}

#[test]
fn creating_empty_subkey_opens_a_new_handle_and_null_create_is_invalid() {
    let (mut registry, mut process) = fixture();
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegCreateKeyExA",
            &[HKLM, 0, 0, 0, 0, ALL_ACCESS, 0, 0x1100, 0x1104]
        ),
        87
    );
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegCreateKeyExA",
            &[HKLM, 0x1000, 0, 0, 0, ALL_ACCESS, 0, 0x1100, 0x1104]
        ),
        0
    );
    let handle = read_u32(&process, 0x1100).unwrap();
    assert_ne!(handle, HKLM);
    assert_eq!(read_u32(&process, 0x1104).unwrap(), 2);
    assert_eq!(
        call(&mut registry, &mut process, "RegCloseKey", &[handle]),
        0
    );
    assert_eq!(
        call(
            &mut registry,
            &mut process,
            "RegOpenKeyExA",
            &[HKLM, 0, 0, 1, 0x1100]
        ),
        0
    );
}
