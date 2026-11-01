use super::*;
use crate::SparseMemory;
fn call(k: &mut Kernel, p: &mut Process<SparseMemory>, dll: &str, name: &str, a: &[u32]) -> u32 {
    k.call(p, resolve(dll, name).unwrap().id, a).unwrap()
}
#[test]
fn system_outputs_have_windows_layouts_and_precise_failures() {
    let mut k = Kernel::default();
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap();
    p.map(0x1000, &[0xaa; 256], true).unwrap();
    call(&mut k, &mut p, "kernel32", "GlobalMemoryStatus", &[0x1000]);
    let mut bytes = [0; 32];
    p.read(0x1000, &mut bytes).unwrap();
    let fields: Vec<_> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    assert_eq!(fields[0], 32);
    assert_eq!(fields[2], 0x1000000);
    assert_eq!(fields[3], 0x1000000 - 256);
    k.set_wall_time(0.).unwrap();
    call(
        &mut k,
        &mut p,
        "kernel32",
        "GetSystemTimeAsFileTime",
        &[0x1020],
    );
    let mut stamp = [0; 8];
    p.read(0x1020, &mut stamp).unwrap();
    assert_eq!(u64::from_le_bytes(stamp), 116444736000000000);
    assert!(k
        .call(
            &mut p,
            resolve("kernel32", "GetSystemTimeAsFileTime").unwrap().id,
            &[0x1020]
        )
        .is_err());
    assert_eq!(
        call(&mut k, &mut p, "winmm", "timeGetDevCaps", &[0x1030, 7]),
        97
    );
    assert_eq!(
        call(&mut k, &mut p, "winmm", "timeGetDevCaps", &[0x1030, 8]),
        0
    );
    p.read(0x1030, &mut stamp).unwrap();
    assert_eq!(stamp, [1, 0, 0, 0, 255, 255, 0, 0]);
    p.write(0x1040, b"__MSVCRT_HEAP_SELECT\0").unwrap();
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "kernel32",
            "GetEnvironmentVariableA",
            &[0x1040, 0x1060, 16]
        ),
        0
    );
    assert_eq!(k.thread().error, 203);
    p.read(0x1060, &mut stamp).unwrap();
    assert_eq!(stamp, [0xaa; 8]);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "kernel32",
            "FormatMessageA",
            &[0xa00, 0x400000, 1, 0, 0x1080, 128, 0]
        ),
        0
    );
    assert_eq!(k.thread().error, 1813);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "kernel32",
            "FormatMessageA",
            &[0x1300, 0, 2, 0, 0x1080, 0, 0]
        ),
        44
    );
    let mut ptr = [0; 4];
    p.read(0x1080, &mut ptr).unwrap();
    let ptr = u32::from_le_bytes(ptr);
    assert_eq!(
        p.read_c_string(ptr, 100).unwrap(),
        b"The system cannot find the file specified.\r\n"
    );
    assert_eq!(call(&mut k, &mut p, "kernel32", "LocalFree", &[ptr]), 0);
    p.write(0x1000, &60u32.to_le_bytes()).unwrap();
    assert_eq!(
        call(&mut k, &mut p, "shell32", "ShellExecuteExA", &[0x1000]),
        0
    );
    assert_eq!(k.thread().error, 1155);
    let mut field = [0; 4];
    p.read(0x1020, &mut field).unwrap();
    assert_eq!(
        u32::from_le_bytes(field),
        31,
        "hInstApp reports SE_ERR_NOASSOC"
    );
    p.read(0x1038, &mut field).unwrap();
    assert_eq!(
        u32::from_le_bytes(field),
        0,
        "failed shell launch has no process handle"
    );
}
