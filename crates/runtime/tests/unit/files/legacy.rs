use super::super::tests::{call, fixture, string};
use super::*;

#[test]
fn legacy_music_probe_reads_size_without_loading_or_leaking_the_track() {
    let (mut files, mut process) = fixture();
    files
        .add_file_source(
            "Music/Modes/Nhood/latin4.mp3",
            2_000_000,
            Rc::new(|_, _| panic!("a size probe must not decode or load the MP3")),
        )
        .unwrap();
    string(&mut process, 0x1000, "./Music\\Modes\\Nhood\\latin4.mp3");
    let before = files.handles.len();
    let (handle, error) = call(&mut files, &mut process, "OpenFile", &[0x1000, 0x1100, 0]);
    assert_ne!(handle, u32::MAX);
    assert_eq!(error, 0);
    assert_eq!(
        call(&mut files, &mut process, "GetFileSize", &[handle, 0]),
        (2_000_000, 0)
    );
    assert_eq!(call(&mut files, &mut process, "_lclose", &[handle]), (0, 0));
    assert_eq!(files.handles.len(), before);
    assert_eq!(
        call(&mut files, &mut process, "_lclose", &[handle]),
        (u32::MAX, 6)
    );
    assert_eq!(
        call(&mut files, &mut process, "GetFileSize", &[handle, 0]),
        (u32::MAX, 6)
    );
    let (reopened, error) = call(&mut files, &mut process, "OpenFile", &[0, 0x1100, 0x8000]);
    assert_eq!(error, 0);
    assert_eq!(
        call(&mut files, &mut process, "CloseHandle", &[reopened]),
        (1, 0)
    );
}

#[test]
fn legacy_open_reports_errors_and_preserves_parse_exist_and_write_semantics() {
    let (mut files, mut process) = fixture();
    string(&mut process, 0x1000, "GameData/missing.dat");
    assert_eq!(
        call(&mut files, &mut process, "OpenFile", &[0x1000, 0x1100, 0]),
        (u32::MAX, 2)
    );
    let mut record = [0; 136];
    process.read(0x1100, &mut record).unwrap();
    assert_eq!(&record[..4], &[136, 1, 2, 0]);
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "OpenFile",
            &[0x1000, 0x1100, 0x100]
        ),
        (0, 0)
    );
    assert!(files.handles.is_empty());
    let (handle, error) = call(&mut files, &mut process, "OpenFile", &[0, 0x1100, 0x9000]);
    assert_eq!(error, 0);
    string(&mut process, 0x1200, "data");
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WriteFile",
            &[handle, 0x1200, 4, 0x1300, 0]
        ),
        (1, 0)
    );
    assert_eq!(call(&mut files, &mut process, "_lclose", &[handle]), (0, 0));
    assert_ne!(
        call(
            &mut files,
            &mut process,
            "OpenFile",
            &[0x1000, 0x1100, 0x4000]
        )
        .0,
        u32::MAX
    );
    assert!(files.handles.is_empty());
    assert_eq!(
        files.file_bytes("GameData/missing.dat").unwrap().unwrap(),
        b"data"
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "OpenFile",
            &[0x1000, 0x1100, 0x200]
        ),
        (1, 0)
    );
    assert!(files.file_bytes("GameData/missing.dat").unwrap().is_none());
    string(
        &mut process,
        0x1000,
        &format!("GameData/{}", "x".repeat(128)),
    );
    assert_eq!(
        call(&mut files, &mut process, "OpenFile", &[0x1000, 0x1100, 0]),
        (u32::MAX, 206)
    );
    assert!(files.handles.is_empty());
}
