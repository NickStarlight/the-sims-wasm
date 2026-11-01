use super::{journal::Change, tests::*};

#[test]
fn frame_matches_original_sms1_bytes_and_restores_partial_writes() {
    let (mut files, mut process) = fixture();
    let handle = open(&mut files, &mut process, 0xc0000000);
    call(
        &mut files,
        &mut process,
        "SetFilePointer",
        &[handle, 4, 0, 0],
    );
    process.write(0x1100, b"XY").unwrap();
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WriteFile",
            &[handle, 0x1100, 2, 0x1200, 0]
        ),
        (1, 0)
    );
    let bytes = files.take_file_changes();
    // Original JS JSON.stringify metadata and standard CRC32, independently computed.
    let mut expected = vec![83, 77, 83, 49, 69, 0, 0, 0, 67, 0, 0, 0, 161, 61, 94, 236];
    expected.extend_from_slice(
        br#"{"kind":"write","path":"gamedata/objects.far","offset":4,"size":10}XY"#,
    );
    assert_eq!(bytes, expected);
    assert!(files.take_file_changes().is_empty());
    let (mut restored, _) = fixture();
    assert_eq!(restored.restore_journal(&bytes).unwrap(), bytes.len());
    assert_eq!(
        &*restored
            .file_bytes("GameData/Objects.far")
            .unwrap()
            .unwrap(),
        b"FAR!XYcdef"
    );
}

#[test]
fn torn_tail_is_recoverable_but_corruption_is_not() {
    let (mut files, _) = fixture();
    files
        .record(
            Change::Replace {
                path: "save.iff".into(),
            },
            b"saved",
        )
        .unwrap();
    let first = files.take_file_changes();
    files
        .record(
            Change::Delete {
                path: "save.iff".into(),
            },
            &[],
        )
        .unwrap();
    let second = files.take_file_changes();
    for length in 1..second.len() {
        let mut bytes = first.clone();
        bytes.extend_from_slice(&second[..length]);
        let (mut restored, _) = fixture();
        assert_eq!(restored.restore_journal(&bytes).unwrap(), first.len());
        assert_eq!(
            &*restored.file_bytes("save.iff").unwrap().unwrap(),
            b"saved"
        );
    }
    let mut corrupt = first.clone();
    *corrupt.last_mut().unwrap() ^= 1;
    assert!(files
        .restore_journal(&corrupt)
        .unwrap_err()
        .to_string()
        .contains("checksum"));
    corrupt[0] = 0;
    assert!(files
        .restore_journal(&corrupt)
        .unwrap_err()
        .to_string()
        .contains("frame"));
}

#[test]
fn copy_move_delete_and_orphan_writes_replay_without_resurrecting_deleted_files() {
    let (mut files, mut process) = fixture();
    let old = open(&mut files, &mut process, 0xc0000000);
    string(&mut process, 0x1300, "copy.far");
    assert_eq!(
        call(&mut files, &mut process, "CopyFileA", &[0x1000, 0x1300, 1]),
        (1, 0)
    );
    assert_eq!(
        call(&mut files, &mut process, "CopyFileA", &[0x1000, 0x1300, 1]),
        (0, 80)
    );
    string(&mut process, 0x1400, "moved.far");
    assert_eq!(
        call(&mut files, &mut process, "MoveFileA", &[0x1300, 0x1400]),
        (1, 0)
    );
    assert_eq!(
        call(&mut files, &mut process, "DeleteFileA", &[0x1000]),
        (1, 0)
    );
    process.write(0x1100, b"gone").unwrap();
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WriteFile",
            &[old, 0x1100, 4, 0x1200, 0]
        ),
        (1, 0)
    );
    let journal = files.take_file_changes();
    let (mut restored, _) = fixture();
    restored.restore_journal(&journal).unwrap();
    assert!(restored
        .file_bytes("GameData/Objects.far")
        .unwrap()
        .is_none());
    assert!(restored.file_bytes("copy.far").unwrap().is_none());
    assert_eq!(
        &*restored.file_bytes("moved.far").unwrap().unwrap(),
        b"FAR!abcdef"
    );
}
