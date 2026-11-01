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

#[test]
#[ignore = "manual replay throughput benchmark"]
fn benchmark_repeated_journal_writes() {
    let (mut source, _) = fixture();
    source
        .record(
            Change::Replace {
                path: "bench.iff".into(),
            },
            &vec![0x5a; 16 * 1024 * 1024],
        )
        .unwrap();
    for offset in 0..200 {
        source
            .record(
                Change::Write {
                    path: "bench.iff".into(),
                    offset,
                    size: 16 * 1024 * 1024,
                },
                &[0x33],
            )
            .unwrap();
    }
    let journal = source.take_file_changes();
    let start = std::time::Instant::now();
    for _ in 0..5 {
        let (mut restored, _) = fixture();
        assert_eq!(restored.restore_journal(&journal).unwrap(), journal.len());
        let bytes = restored.file_bytes("bench.iff").unwrap().unwrap();
        assert_eq!(&bytes[..200], &[0x33; 200]);
        assert_eq!(bytes[200], 0x5a);
        std::hint::black_box(restored);
    }
    eprintln!(
        "journal replay 5 x (16 MiB + 200 small writes): {:?}",
        start.elapsed()
    );
}

#[test]
fn replay_reuses_private_overlay_and_zeroes_regrown_bytes() {
    let (mut files, _) = fixture();
    files
        .record(
            Change::Replace {
                path: "save.iff".into(),
            },
            b"abcdef",
        )
        .unwrap();
    let initial = files.take_file_changes();
    files.restore_journal(&initial).unwrap();
    let pointer = files.entries["save.iff"]
        .borrow()
        .overlay
        .as_ref()
        .unwrap()
        .as_ptr();
    files
        .record(
            Change::Write {
                path: "save.iff".into(),
                offset: 1,
                size: 6,
            },
            b"X",
        )
        .unwrap();
    files
        .record(
            Change::Resize {
                path: "save.iff".into(),
                size: 3,
            },
            &[],
        )
        .unwrap();
    files
        .record(
            Change::Resize {
                path: "save.iff".into(),
                size: 6,
            },
            &[],
        )
        .unwrap();
    let changes = files.take_file_changes();
    files.restore_journal(&changes).unwrap();
    assert_eq!(
        files.entries["save.iff"]
            .borrow()
            .overlay
            .as_ref()
            .unwrap()
            .as_ptr(),
        pointer
    );
    assert_eq!(files.file_bytes("save.iff").unwrap().unwrap(), b"aXc\0\0\0");
    let held = files.entries["save.iff"].clone();
    files
        .record(
            Change::Write {
                path: "save.iff".into(),
                offset: 0,
                size: 6,
            },
            b"Z",
        )
        .unwrap();
    let changes = files.take_file_changes();
    files.restore_journal(&changes).unwrap();
    assert_eq!(&*held.borrow().bytes().unwrap(), b"aXc\0\0\0");
    assert_eq!(files.file_bytes("save.iff").unwrap().unwrap(), b"ZXc\0\0\0");
}

#[test]
fn compacted_journal_preserves_moves_copies_deletions_and_partial_writes() {
    let (mut files, _) = fixture();
    files
        .record(
            Change::Copy {
                path: "gamedata/objects.far".into(),
                to: "copy.far".into(),
            },
            &[],
        )
        .unwrap();
    files
        .record(
            Change::Move {
                path: "copy.far".into(),
                to: "UserData/Moved.far".into(),
            },
            &[],
        )
        .unwrap();
    files
        .record(
            Change::Delete {
                path: "gamedata/objects.far".into(),
            },
            &[],
        )
        .unwrap();
    files
        .record(
            Change::Write {
                path: "UserData/Moved.far".into(),
                offset: 4,
                size: 10,
            },
            b"XY",
        )
        .unwrap();
    let journal = files.take_file_changes();
    files.restore_journal(&journal).unwrap();
    let compact = files.compact_journal(&journal).unwrap();
    crate::validate_save_journal(&compact).unwrap();
    let (mut restored, _) = fixture();
    restored.restore_journal(&compact).unwrap();
    for path in ["gamedata/objects.far", "copy.far", "UserData/Moved.far"] {
        assert_eq!(
            restored.file_bytes(path).unwrap(),
            files.file_bytes(path).unwrap()
        );
    }
    assert_eq!(restored.compact_journal(&compact).unwrap(), compact);
    assert!(files
        .compact_journal(&journal[..journal.len() - 1])
        .is_err());
}

#[test]
fn compaction_bounds_repeated_save_history() {
    let (mut files, _) = fixture();
    for value in 0..300u32 {
        files
            .record(
                Change::Replace {
                    path: "UserData/House01.iff".into(),
                },
                &vec![value as u8; 65536],
            )
            .unwrap();
    }
    let journal = files.take_file_changes();
    files.restore_journal(&journal).unwrap();
    let snapshot = files.compact_journal(&journal).unwrap();
    assert!(journal.len() > 16 * 1024 * 1024);
    assert!(snapshot.len() < 66000);
    let (mut restored, _) = fixture();
    restored.restore_journal(&snapshot).unwrap();
    assert_eq!(
        restored
            .file_bytes("userdata/house01.iff")
            .unwrap()
            .unwrap(),
        vec![299u32 as u8; 65536]
    );
    assert_eq!(restored.names["userdata/house01.iff"], "House01.iff");
    assert_eq!(restored.names["userdata"], "UserData");
}
