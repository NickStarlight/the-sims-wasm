use crate::{
    kernel::{resolve, Kernel},
    modules::tests::configuration,
    Process, SparseMemory,
};

fn call(kernel: &mut Kernel, process: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    kernel
        .call(process, resolve("kernel32.dll", name).unwrap().id, args)
        .unwrap()
}
fn setup() -> (Kernel, Process<SparseMemory>) {
    let mut kernel = Kernel::default();
    kernel.install_modules(&configuration()).unwrap();
    let mut process = Process::new(SparseMemory::default(), 0x20000, 0x100000).unwrap();
    process.map(0x10000, &[0; 4096], true).unwrap();
    (kernel, process)
}

#[test]
fn extended_library_loads_use_the_catalog_and_reject_unimplemented_mapping_modes() {
    let (mut kernel, mut process) = setup();
    process.write(0x10000, b"GONZO\0").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "LoadLibraryExA",
            &[0x10000, 0, 0]
        ),
        0x10000000
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "LoadLibraryExA",
            &[0x10000, 0, 2]
        ),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 87);
    assert_eq!(
        call(&mut kernel, &mut process, "FreeLibrary", &[0x10000000]),
        1
    );
    assert_eq!(
        call(&mut kernel, &mut process, "FreeLibrary", &[0x10000000]),
        0
    );
    process.write(0x10000, b"missing.dll\0").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "LoadLibraryExA",
            &[0x10000, 0, 0]
        ),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 126);
}

#[test]
fn module_calls_resolve_names_ordinals_and_set_last_error() {
    let (mut kernel, mut process) = setup();
    process.write(0x10000, b"GONZO\0").unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleA", &[0]),
        0x400000
    );
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleA", &[0x10000]),
        0x10000000
    );
    assert_eq!(
        call(&mut kernel, &mut process, "LoadLibraryA", &[0x10000]),
        0x10000000
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetProcAddress",
            &[0x10000000, 1]
        ),
        0x10001000
    );
    process.write(0x10000, b"Forward\0").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetProcAddress",
            &[0x10000000, 0x10000]
        ),
        0x800100
    );
    assert_eq!(
        call(&mut kernel, &mut process, "FreeLibrary", &[0x10000000]),
        1
    );
    assert_eq!(
        call(&mut kernel, &mut process, "FreeLibrary", &[0x10000000]),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 6);
    process.write(0x10000, b"unknown.dll\0").unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "LoadLibraryA", &[0x10000]),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 126);
    process.write(0x10000, b"local\0").unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetProcAddress",
            &[0x10000000, 0x10000]
        ),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 127);
}

#[test]
fn filenames_and_wide_names_respect_buffer_units_and_termination() {
    let (mut kernel, mut process) = setup();
    let wide: Vec<_> = "GoNzO.dll\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    process.write(0x10000, &wide).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleW", &[0x10000]),
        0x10000000
    );
    let expected = b"C:\\Games\\Sims.exe";
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetModuleFileNameA",
            &[0, 0x10100, 100]
        ),
        expected.len() as u32
    );
    let mut bytes = vec![0; expected.len() + 1];
    process.read(0x10100, &mut bytes).unwrap();
    assert_eq!(&bytes[..expected.len()], expected);
    assert_eq!(bytes[expected.len()], 0);
    process.write(0x10200, &[0xcc; 16]).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetModuleFileNameW",
            &[0, 0x10200, 4]
        ),
        4
    );
    let mut output = [0; 10];
    process.read(0x10200, &mut output).unwrap();
    assert_eq!(output, [b'C', 0, b':', 0, b'\\', 0, 0, 0, 0xcc, 0xcc]);
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 122);
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleFileNameA", &[0, 0, 0]),
        0
    );
    assert!(kernel.install_modules("not json").is_err());
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleA", &[0]),
        0x400000
    );
}

#[test]
fn boot_export_lookup_does_not_change_module_references_or_last_error() {
    let (mut kernel, mut process) = setup();
    call(&mut kernel, &mut process, "SetLastError", &[123]);
    assert_eq!(
        kernel.export_address("GONZO", Some("Forward"), None),
        Some(0x800100)
    );
    assert_eq!(
        kernel.export_address("gonzo.dll", None, Some(1)),
        Some(0x10001000)
    );
    assert_eq!(
        kernel.export_address("gonzo.dll", Some("missing"), None),
        None
    );
    assert_eq!(
        kernel.export_address("missing.dll", Some("HeapAlloc"), None),
        None
    );
    assert_eq!(
        kernel.export_address("gonzo.dll", Some("Local"), Some(1)),
        None
    );
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 123);
    assert_eq!(
        call(&mut kernel, &mut process, "FreeLibrary", &[0x10000000]),
        0
    );
}

#[test]
fn terminated_module_names_can_end_at_mapping_boundary() {
    let (mut kernel, mut process) = setup();
    process.map(0x12000, b"Gonzo\0", false).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleA", &[0x12000]),
        0x10000000
    );
    let wide: Vec<_> = "Gonzo\0"
        .encode_utf16()
        .flat_map(u16::to_le_bytes)
        .collect();
    process.map(0x13000, &wide, false).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetModuleHandleW", &[0x13000]),
        0x10000000
    );
}
