use super::*;
use crate::SparseMemory;
pub(super) fn fixture() -> (Files, Process<SparseMemory>) {
    let mut files = Files::default();
    files
        .add_file("GameData/Objects.far", b"FAR!abcdef")
        .unwrap();
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x1000, &vec![0; 4096], true).unwrap();
    (files, process)
}
pub(super) fn string(process: &mut Process<SparseMemory>, pointer: u32, text: &str) {
    let mut bytes = text.as_bytes().to_vec();
    bytes.push(0);
    process.write(pointer, &bytes).unwrap();
}
pub(super) fn call(
    files: &mut Files,
    process: &mut Process<SparseMemory>,
    name: &str,
    args: &[u32],
) -> (u32, u32) {
    files.call(process, name, args).unwrap()
}
pub(super) fn open(files: &mut Files, process: &mut Process<SparseMemory>, access: u32) -> u32 {
    string(process, 0x1000, "C:\\Sims\\GAMEDATA\\Objects.FAR");
    let (handle, error) = call(
        files,
        process,
        "CreateFileA",
        &[0x1000, access, 3, 0, 3, 0, 0],
    );
    assert_eq!(error, 0);
    handle
}
#[test]
fn far_reads_seek_eof_and_missing_files_use_windows_results() {
    let (mut files, mut process) = fixture();
    let handle = open(&mut files, &mut process, 0x80000000);
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "ReadFile",
            &[handle, 0x1100, 4, 0x1200, 0]
        ),
        (1, 0)
    );
    let mut bytes = [0; 4];
    process.read(0x1100, &mut bytes).unwrap();
    assert_eq!(&bytes, b"FAR!");
    assert_eq!(read_u32(&process, 0x1200).unwrap(), 4);
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "SetFilePointer",
            &[handle, (-2i32) as u32, 0, 2]
        ),
        (8, 0)
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "ReadFile",
            &[handle, 0x1100, 4, 0x1200, 0]
        ),
        (1, 0)
    );
    assert_eq!(read_u32(&process, 0x1200).unwrap(), 2);
    process.read(0x1100, &mut bytes[..2]).unwrap();
    assert_eq!(&bytes[..2], b"ef");
    call(
        &mut files,
        &mut process,
        "ReadFile",
        &[handle, 0x1100, 4, 0x1200, 0],
    );
    assert_eq!(read_u32(&process, 0x1200).unwrap(), 0);
    string(&mut process, 0x1000, "GameData/missing.far");
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "CreateFileA",
            &[0x1000, 0x80000000, 3, 0, 3, 0, 0]
        ),
        (u32::MAX, 2)
    );
}
#[test]
fn writes_are_shared_overlay_and_do_not_modify_preloaded_backing() {
    let (mut files, mut process) = fixture();
    let first = open(&mut files, &mut process, 0xc0000000);
    let second = open(&mut files, &mut process, 0x80000000);
    process.write(0x1100, b"NEW!").unwrap();
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WriteFile",
            &[first, 0x1100, 4, 0x1200, 0]
        ),
        (1, 0)
    );
    call(
        &mut files,
        &mut process,
        "ReadFile",
        &[second, 0x1100, 4, 0x1200, 0],
    );
    let mut bytes = [0; 4];
    process.read(0x1100, &mut bytes).unwrap();
    assert_eq!(&bytes, b"NEW!");
    assert_eq!(
        &*files.entries["gamedata/objects.far"].borrow().original,
        b"FAR!abcdef"
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WriteFile",
            &[second, 0x1100, 4, 0x1200, 0]
        ),
        (0, 5)
    );
    assert_eq!(
        call(&mut files, &mut process, "DeleteFileA", &[0x1000]),
        (1, 0)
    );
    assert_eq!(
        call(&mut files, &mut process, "GetFileAttributesA", &[0x1000]),
        (u32::MAX, 2)
    );
    assert_eq!(
        call(&mut files, &mut process, "GetFileSize", &[first, 0]),
        (10, 0)
    );
}
#[test]
fn invalid_guest_output_does_not_advance_cursor() {
    let (mut files, mut process) = fixture();
    let handle = open(&mut files, &mut process, 0x80000000);
    assert_eq!(
        files.call(&mut process, "ReadFile", &[handle, 0x9000, 4, 0x1200, 0]),
        Err(Error::Unmapped)
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "SetFilePointer",
            &[handle, 0, 0, 1]
        ),
        (0, 0)
    );
}
#[test]
fn directory_find_records_and_handle_lifetimes_are_explicit() {
    let (mut files, mut process) = fixture();
    files.add_file("GameData/config", b"x").unwrap();
    string(&mut process, 0x1000, "gamedata\\*.*");
    let (handle, error) = call(
        &mut files,
        &mut process,
        "FindFirstFileA",
        &[0x1000, 0x1300],
    );
    assert_eq!(error, 0);
    for expected in [".", ".."] {
        assert_eq!(read_string(&process, 0x1300 + 44).unwrap(), expected);
        assert_eq!(read_u32(&process, 0x1300).unwrap(), 0x10);
        assert_eq!(
            call(&mut files, &mut process, "FindNextFileA", &[handle, 0x1300]),
            (1, 0)
        );
    }
    assert_eq!(read_string(&process, 0x1300 + 44).unwrap(), "config");
    assert_eq!(
        call(&mut files, &mut process, "FindNextFileA", &[handle, 0x1300]),
        (1, 0)
    );
    assert_eq!(read_string(&process, 0x1300 + 44).unwrap(), "Objects.far");
    assert_eq!(read_u32(&process, 0x1300 + 32).unwrap(), 10);
    assert_eq!(
        call(&mut files, &mut process, "FindNextFileA", &[handle, 0x1300]),
        (0, 18)
    );
    assert_eq!(
        call(&mut files, &mut process, "FindClose", &[handle]),
        (1, 0)
    );
    assert_eq!(
        call(&mut files, &mut process, "FindClose", &[handle]),
        (0, 6)
    );
}
#[test]
fn ini_keys_defaults_and_overlay_writes_are_case_insensitive() {
    let (mut files, mut process) = fixture();
    files
        .add_file("sims.ini", b"[Game]\r\nSpeed=3\r\nName=Original\r\n")
        .unwrap();
    for (address, text) in [
        (0x1000, "GAME"),
        (0x1040, "speed"),
        (0x1080, "sims.ini"),
        (0x10c0, "9"),
    ] {
        string(&mut process, address, text);
    }
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "GetPrivateProfileIntA",
            &[0x1000, 0x1040, 7, 0x1080]
        ),
        (3, 0)
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "WritePrivateProfileStringA",
            &[0x1000, 0x1040, 0x10c0, 0x1080]
        ),
        (1, 0)
    );
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "GetPrivateProfileIntA",
            &[0x1000, 0x1040, 7, 0x1080]
        ),
        (9, 0)
    );
    string(&mut process, 0x1040, "missing");
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "GetPrivateProfileStringA",
            &[0x1000, 0x1040, 0x10c0, 0x1100, 64, 0x1080]
        ),
        (1, 0)
    );
    assert_eq!(read_string(&process, 0x1100).unwrap(), "9");
}
#[test]
fn independent_process_file_overlays_do_not_leak() {
    let (mut files, mut process) = fixture();
    let handle = open(&mut files, &mut process, 0xc0000000);
    process.write(0x1100, b"X").unwrap();
    call(
        &mut files,
        &mut process,
        "WriteFile",
        &[handle, 0x1100, 1, 0x1200, 0],
    );
    let (other, _) = fixture();
    assert_eq!(
        other.entries["gamedata/objects.far"]
            .borrow()
            .bytes()
            .unwrap()
            .as_ref(),
        b"FAR!abcdef"
    );
}

#[test]
fn extension_searches_do_not_insert_dot_directories() {
    let (mut files, mut process) = fixture();
    string(&mut process, 0x1000, "gamedata/*.far");
    let (handle, error) = call(
        &mut files,
        &mut process,
        "FindFirstFileA",
        &[0x1000, 0x1300],
    );
    assert_eq!(error, 0);
    assert_eq!(read_string(&process, 0x1300 + 44).unwrap(), "Objects.far");
    assert_eq!(
        call(&mut files, &mut process, "FindNextFileA", &[handle, 0x1300]),
        (0, 18)
    );
}

fn found_names(
    files: &mut Files,
    process: &mut Process<SparseMemory>,
    pattern: &str,
) -> Vec<String> {
    string(process, 0x1000, pattern);
    let (handle, error) = call(files, process, "FindFirstFileA", &[0x1000, 0x1300]);
    assert_eq!(error, 0);
    let mut names = Vec::new();
    loop {
        names.push(read_string(process, 0x1300 + 44).unwrap());
        let result = call(files, process, "FindNextFileA", &[handle, 0x1300]);
        if result == (0, 18) {
            break;
        }
        assert_eq!(result, (1, 0));
    }
    assert_eq!(call(files, process, "FindClose", &[handle]), (1, 0));
    names
}

#[test]
fn searches_preserve_source_names_while_matching_case_insensitively() {
    let (_, mut process) = fixture();
    let mut files = Files::default();
    files
        .add_file("C:\\sImS\\UserData\\Neighborhood.iff", b"hood")
        .unwrap();
    files
        .add_file("UserData2/Characters/Bella.IFF", b"bella")
        .unwrap();
    // A later import with different parent spelling does not rename that directory.
    files
        .add_file("userdata/Characters/Mortimer.IFF", b"mortimer")
        .unwrap();
    assert_eq!(
        found_names(&mut files, &mut process, "USERDATA*"),
        ["UserData", "UserData2"],
        "Complete recognizes neighborhood 1 by the exact UserData spelling"
    );
    assert_eq!(
        found_names(&mut files, &mut process, "uSeRdAtA/CHARACTERS/*.iff"),
        ["Mortimer.IFF"]
    );
    assert_eq!(
        found_names(&mut files, &mut process, "USERDATA/nEIGHBORHOOD.IFF"),
        ["Neighborhood.iff"]
    );
    string(&mut process, 0x1000, "USERDATA/characters/mortimer.iff");
    let (handle, error) = call(
        &mut files,
        &mut process,
        "CreateFileA",
        &[0x1000, 0x80000000, 3, 0, 3, 0, 0],
    );
    assert_eq!(error, 0);
    assert_eq!(
        call(&mut files, &mut process, "GetFileSize", &[handle, 0]),
        (8, 0)
    );
}

#[test]
fn created_and_renamed_file_spelling_survives_journal_replay() {
    let (mut files, mut process) = fixture();
    string(&mut process, 0x1000, "GameData/House.IFF");
    let (handle, error) = call(
        &mut files,
        &mut process,
        "CreateFileA",
        &[0x1000, 0x40000000, 3, 0, 2, 0, 0],
    );
    assert_eq!(error, 0);
    call(&mut files, &mut process, "CloseHandle", &[handle]);
    string(&mut process, 0x1080, "GameData/CopiedHouse.IFF");
    assert_eq!(
        call(&mut files, &mut process, "CopyFileA", &[0x1000, 0x1080, 1]),
        (1, 0)
    );
    string(&mut process, 0x1000, "GameData/MovedHouse.IFF");
    assert_eq!(
        call(&mut files, &mut process, "MoveFileA", &[0x1080, 0x1000]),
        (1, 0)
    );
    string(&mut process, 0x1000, "gamedata/objects.far");
    assert_eq!(
        call(&mut files, &mut process, "DeleteFileA", &[0x1000]),
        (1, 0)
    );
    string(&mut process, 0x1000, "GameData/OBJECTS.FAR");
    assert_eq!(
        call(
            &mut files,
            &mut process,
            "CreateFileA",
            &[0x1000, 0x40000000, 3, 0, 2, 0, 0]
        )
        .1,
        0
    );
    let journal = files.take_file_changes();
    let (mut restored, _) = fixture();
    assert_eq!(restored.restore_journal(&journal).unwrap(), journal.len());
    for files in [&mut files, &mut restored] {
        assert_eq!(
            found_names(files, &mut process, "gamedata/*.*"),
            [".", "..", "House.IFF", "MovedHouse.IFF", "OBJECTS.FAR"]
        );
    }
}

#[test]
fn deferred_files_read_ranges_preserve_overlays_and_restore_journals() {
    let (mut files, mut process) = fixture();
    files.entries.clear();
    let reads = Rc::new(RefCell::new(Vec::new()));
    let observed = reads.clone();
    files
        .add_file_source(
            "GameData/Objects.far",
            10,
            Rc::new(move |offset, length| {
                observed.borrow_mut().push((offset, length));
                Ok(b"FAR!abcdef"[offset as usize..offset as usize + length].to_vec())
            }),
        )
        .unwrap();
    let handle = open(&mut files, &mut process, 0xc0000000);
    assert_eq!(
        call(&mut files, &mut process, "GetFileSize", &[handle, 0]),
        (10, 0)
    );
    string(&mut process, 0x1000, "GameData/*.far");
    call(
        &mut files,
        &mut process,
        "FindFirstFileA",
        &[0x1000, 0x1300],
    );
    assert!(
        reads.borrow().is_empty(),
        "metadata must not read file contents"
    );
    call(
        &mut files,
        &mut process,
        "SetFilePointer",
        &[handle, 8, 0, 0],
    );
    call(
        &mut files,
        &mut process,
        "ReadFile",
        &[handle, 0x1100, 4, 0x1200, 0],
    );
    assert_eq!(reads.borrow().as_slice(), &[(8, 2)]);
    assert_eq!(read_u32(&process, 0x1200).unwrap(), 2);
    call(
        &mut files,
        &mut process,
        "ReadFile",
        &[handle, 0x1100, 4, 0x1200, 0],
    );
    assert_eq!(read_u32(&process, 0x1200).unwrap(), 0);
    assert_eq!(
        reads.borrow().as_slice(),
        &[(8, 2)],
        "EOF must not invoke the reader"
    );
    process.write(0x1100, b"X").unwrap();
    call(
        &mut files,
        &mut process,
        "SetFilePointer",
        &[handle, 0, 0, 0],
    );
    call(
        &mut files,
        &mut process,
        "WriteFile",
        &[handle, 0x1100, 1, 0x1200, 0],
    );
    let count = reads.borrow().len();
    assert_eq!(
        files.file_bytes("GameData/Objects.far").unwrap().unwrap(),
        b"XAR!abcdef"
    );
    assert_eq!(
        reads.borrow().len(),
        count,
        "writes must read immutable backing only once"
    );
    let changes = files.take_file_changes();
    let (mut restored, _) = fixture();
    restored.restore_journal(&changes).unwrap();
    assert_eq!(
        restored
            .file_bytes("GameData/Objects.far")
            .unwrap()
            .unwrap(),
        b"XAR!abcdef"
    );
}

#[test]
fn failed_deferred_reads_do_not_advance_cursors_or_cache_partial_contents() {
    let (mut files, mut process) = fixture();
    files.entries.clear();
    let failed = Rc::new(std::cell::Cell::new(true));
    let state = failed.clone();
    files
        .add_file_source(
            "GameData/Objects.far",
            10,
            Rc::new(move |offset, length| {
                Ok(if state.get() {
                    vec![]
                } else {
                    b"FAR!abcdef"[offset as usize..offset as usize + length].to_vec()
                })
            }),
        )
        .unwrap();
    let handle = open(&mut files, &mut process, 0x80000000);
    assert!(files
        .call(&mut process, "ReadFile", &[handle, 0x1100, 4, 0x1200, 0])
        .is_err());
    assert_eq!(files.handles[&handle].position, 0);
    failed.set(false);
    call(
        &mut files,
        &mut process,
        "ReadFile",
        &[handle, 0x1100, 4, 0x1200, 0],
    );
    let mut bytes = [0; 4];
    process.read(0x1100, &mut bytes).unwrap();
    assert_eq!(&bytes, b"FAR!");
}
