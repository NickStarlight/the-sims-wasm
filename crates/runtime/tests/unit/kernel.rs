use super::*;
use crate::SparseMemory;

fn call(kernel: &mut Kernel, process: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    kernel
        .call(process, resolve("KERNEL32.dll", name).unwrap().id, args)
        .unwrap()
}
fn process() -> Process<SparseMemory> {
    Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap()
}

#[test]
fn command_line_uses_the_validated_loader_buffer() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let address = 0xd00000;
    let command = b"C:\\Sims\\Sims.exe -skip_intro\0";
    process.map(address, command, true).unwrap();
    kernel.set_command_line(&process, address).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetCommandLineA", &[]),
        address
    );
    assert_eq!(
        process.read_c_string(address, 4096).unwrap(),
        &command[..command.len() - 1]
    );
    assert!(kernel.set_command_line(&process, 0).is_err());
    assert_eq!(
        call(&mut kernel, &mut process, "GetCommandLineA", &[]),
        address
    );
    let unterminated = process.heap_alloc(4096).unwrap();
    process.write(unterminated, &[1; 4096]).unwrap();
    assert!(kernel.set_command_line(&process, unterminated).is_err());
    assert_eq!(
        call(&mut kernel, &mut process, "GetCommandLineA", &[]),
        address
    );
}

#[test]
fn directinput_detection_export_does_not_claim_to_create_devices() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let output = process.heap_alloc(4).unwrap();
    process.write(output, &123u32.to_le_bytes()).unwrap();
    let api = resolve("dinput.dll", "DirectInputCreateA").unwrap();
    assert_eq!(api.arity, 4);
    assert_eq!(api.stack_args, 4);
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0x400000, 0x700, output, 0])
            .unwrap(),
        0x80004001
    );
    let mut bytes = [1; 4];
    process.read(output, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
}

#[test]
fn tls_and_last_error_belong_to_each_thread() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let slot = call(&mut kernel, &mut process, "TlsAlloc", &[]);
    call(&mut kernel, &mut process, "TlsSetValue", &[slot, 123]);
    call(&mut kernel, &mut process, "SetLastError", &[42]);
    kernel.set_thread(2).unwrap();
    assert_eq!(call(&mut kernel, &mut process, "TlsGetValue", &[slot]), 0);
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 0);
    kernel.set_thread(1).unwrap();
    assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 42);
    assert_eq!(call(&mut kernel, &mut process, "TlsGetValue", &[slot]), 123);
    call(&mut kernel, &mut process, "TlsFree", &[slot]);
    assert_eq!(call(&mut kernel, &mut process, "TlsAlloc", &[]), slot);
    assert_eq!(call(&mut kernel, &mut process, "TlsGetValue", &[slot]), 0);
}

#[test]
fn critical_sections_track_recursion_and_reject_unimplemented_waits() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let address = process.heap_alloc(24).unwrap();
    call(
        &mut kernel,
        &mut process,
        "InitializeCriticalSection",
        &[address],
    );
    call(
        &mut kernel,
        &mut process,
        "EnterCriticalSection",
        &[address],
    );
    call(
        &mut kernel,
        &mut process,
        "EnterCriticalSection",
        &[address],
    );
    call(
        &mut kernel,
        &mut process,
        "LeaveCriticalSection",
        &[address],
    );
    kernel.set_thread(2).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "TryEnterCriticalSection",
            &[address]
        ),
        0
    );
    let enter = resolve("kernel32", "EnterCriticalSection").unwrap().id;
    assert!(kernel.call(&mut process, enter, &[address]).is_err());
    kernel.set_thread(1).unwrap();
    call(
        &mut kernel,
        &mut process,
        "LeaveCriticalSection",
        &[address],
    );
    kernel.set_thread(2).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "TryEnterCriticalSection",
            &[address]
        ),
        1
    );
}

#[test]
fn codepage_conversion_sizes_and_roundtrips_windows_1252() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let source = process.heap_alloc(4).unwrap();
    process.write(source, &[b'A', 0x80, 0]).unwrap();
    let wide = process.heap_alloc(6).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "MultiByteToWideChar",
            &[1252, 0, source, u32::MAX, 0, 0]
        ),
        3
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "MultiByteToWideChar",
            &[1252, 0, source, u32::MAX, wide, 3]
        ),
        3
    );
    let mut utf16 = [0; 6];
    process.read(wide, &mut utf16).unwrap();
    assert_eq!(utf16, [65, 0, 0xac, 0x20, 0, 0]);
    let output = process.heap_alloc(3).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WideCharToMultiByte",
            &[0, 0, wide, u32::MAX, output, 3, 0, 0]
        ),
        3
    );
    let mut bytes = [0; 3];
    process.read(output, &mut bytes).unwrap();
    assert_eq!(bytes, [65, 0x80, 0]);
}

#[test]
fn unresolved_apis_and_wrong_argument_counts_are_errors() {
    assert!(resolve("user32.dll", "HeapAlloc").is_none());
    assert!(resolve("kernel32.dll", "UnsupportedApi").is_none());
    let mut kernel = Kernel::default();
    assert!(kernel.call(&mut process(), u32::MAX, &[]).is_err());
    assert!(kernel
        .call(
            &mut process(),
            resolve("kernel32", "HeapAlloc").unwrap().id,
            &[]
        )
        .is_err());
}

#[test]
fn virtual_alloc_dispatch_preserves_reserve_and_commit_modes() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let address = call(
        &mut kernel,
        &mut process,
        "VirtualAlloc",
        &[0, 0x10000, 0x2000, 4],
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "VirtualAlloc",
            &[address, 0x1000, 0x1000, 4]
        ),
        address
    );
    process.write(address, &[0x42]).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "VirtualAlloc",
            &[address, 0x1000, 0x1000, 4]
        ),
        address
    );
    let mut byte = [0];
    process.read(address, &mut byte).unwrap();
    assert_eq!(
        byte,
        [0x42],
        "recommitting memory must preserve its contents"
    );
    call(
        &mut kernel,
        &mut process,
        "VirtualFree",
        &[address, 0, 0x8000],
    );
}

#[test]
fn string_type_a_and_w_share_cp1252_classes_and_word_outputs() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let narrow = process.heap_alloc(4).unwrap();
    process.write(narrow, &[b'A', 0xe9, b'\t', 0]).unwrap();
    let wide = process.heap_alloc(8).unwrap();
    process
        .write(wide, &[b'A', 0, 0xe9, 0, b'\t', 0, 0, 0])
        .unwrap();
    let output_a = process.heap_alloc(8).unwrap();
    let output_w = process.heap_alloc(8).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetStringTypeA",
            &[0x409, 1, narrow, u32::MAX, output_a]
        ),
        1
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetStringTypeW",
            &[1, wide, u32::MAX, output_w]
        ),
        1
    );
    let mut a = [0; 8];
    let mut w = [0; 8];
    process.read(output_a, &mut a).unwrap();
    process.read(output_w, &mut w).unwrap();
    assert_eq!(a, w);
    assert_eq!(u16::from_le_bytes([a[0], a[1]]) & 0x1ff, 0x181);
    assert_eq!(u16::from_le_bytes([a[6], a[7]]) & 0x1ff, 0x20);
}

#[test]
fn import_manifest_identifies_each_supported_thunk() {
    let entries: serde_json::Value = serde_json::from_str(&manifest_json()).unwrap();
    assert_eq!(entries.as_array().unwrap().len(), manifest().len());
    for (index, api) in manifest().iter().enumerate() {
        assert_eq!(api.id as usize, index);
        let entry = &entries[api.id as usize];
        assert_eq!(entry["name"], api.name);
        assert_eq!(entry["arity"], api.arity);
        assert_eq!(entry["dll"], api.dll);
        assert_eq!(entry["stackArgs"], api.stack_args);
    }
}

#[test]
fn precomposed_1252_sizing_and_conversion_preserve_explicit_input_count() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let source = process.heap_alloc(256).unwrap();
    process.write(source, &[0xe9, 0, 0x80, 0xc4]).unwrap();
    let output = process.heap_alloc(512).unwrap();
    for flags in [0, 1, 8, 9] {
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "MultiByteToWideChar",
                &[1252, flags, source, 256, 0, 0]
            ),
            256
        );
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "MultiByteToWideChar",
                &[1252, flags, source, 4, output, 4]
            ),
            4
        );
        let mut bytes = [0; 8];
        process.read(output, &mut bytes).unwrap();
        assert_eq!(bytes, [0xe9, 0, 0, 0, 0xac, 0x20, 0xc4, 0]);
    }
    for flags in [3, 0x10, 0xffff] {
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "MultiByteToWideChar",
                &[1252, flags, source, 4, output, 4]
            ),
            0
        );
        assert_eq!(call(&mut kernel, &mut process, "GetLastError", &[]), 1004);
    }
    // Composite and glyph modes are valid Windows flags, but remain explicitly unsupported.
    for flags in [2, 4] {
        assert!(kernel
            .call(
                &mut process,
                resolve("kernel32", "MultiByteToWideChar").unwrap().id,
                &[1252, flags, source, 4, output, 4]
            )
            .is_err());
    }
}

#[test]
fn composite_wide_conversion_sizes_actual_output_and_reports_replacement() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let source = process.heap_alloc(512).unwrap();
    let destination = process.heap_alloc(256).unwrap();
    let used = process.heap_alloc(4).unwrap();
    process
        .write(source, &[b'e', 0, 1, 3, b'x', 0, 8, 3])
        .unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WideCharToMultiByte",
            &[1252, 0x220, source, 256, 0, 0, 0, used]
        ),
        255
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WideCharToMultiByte",
            &[1252, 0x220, source, 256, destination, 256, 0, used]
        ),
        255
    );
    let mut bytes = [0; 4];
    process.read(destination, &mut bytes).unwrap();
    assert_eq!(bytes, [0xe9, b'x', b'?', 0]);
    process.read(used, &mut bytes).unwrap();
    assert_eq!(bytes, 1u32.to_le_bytes());
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WideCharToMultiByte",
            &[1252, 0x230, source, 256, 0, 0, 0, used]
        ),
        0
    );
    assert_eq!(kernel.thread().error, 1004);
}

#[test]
fn exception_filter_registration_is_process_wide_and_returns_previous() {
    let mut process = Process::new(crate::SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "SetUnhandledExceptionFilter",
            &[0x401000]
        ),
        0
    );
    kernel.set_thread(2).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "SetUnhandledExceptionFilter",
            &[0x402000]
        ),
        0x401000
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "SetUnhandledExceptionFilter",
            &[0]
        ),
        0x402000
    );
    assert_eq!(
        call(
            &mut Kernel::default(),
            &mut process,
            "SetUnhandledExceptionFilter",
            &[0]
        ),
        0
    );
}

#[test]
fn thread_priority_validates_handles_and_preserves_per_thread_state() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let current = 0xffff_fffe;
    assert_eq!(
        call(&mut kernel, &mut process, "GetThreadPriority", &[current]),
        0
    );
    for priority in [-15i32, -2, -1, 0, 1, 2, 15] {
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "SetThreadPriority",
                &[current, priority as u32]
            ),
            1
        );
        assert_eq!(
            call(&mut kernel, &mut process, "GetThreadPriority", &[current]),
            priority as u32
        );
    }
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "SetThreadPriority",
            &[current, 3]
        ),
        0
    );
    assert_eq!(kernel.thread().error, 87);
    assert_eq!(
        call(&mut kernel, &mut process, "GetThreadPriority", &[current]),
        15
    );
    assert_eq!(
        call(&mut kernel, &mut process, "SetThreadPriority", &[1, 0]),
        0
    );
    assert_eq!(kernel.thread().error, 6);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "GetThreadPriority",
            &[0xffff_ffff]
        ),
        0x7fff_ffff
    );
    assert_eq!(kernel.thread().error, 6);
    kernel.set_thread(2).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetThreadPriority", &[current]),
        0
    );
    kernel.set_thread(1).unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "GetThreadPriority", &[current]),
        15
    );
}

#[test]
fn interlocked_operations_wrap_and_return_the_documented_value() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let address = process.heap_alloc(4).unwrap();
    process
        .write(address, &0x7fff_ffffu32.to_le_bytes())
        .unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedIncrement",
            &[address]
        ),
        0x8000_0000
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedDecrement",
            &[address]
        ),
        0x7fff_ffff
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedExchange",
            &[address, u32::MAX]
        ),
        0x7fff_ffff
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedExchangeAdd",
            &[address, 2]
        ),
        u32::MAX
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedCompareExchange",
            &[address, 8, 0]
        ),
        1
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "InterlockedCompareExchange",
            &[address, 8, 1]
        ),
        1
    );
    let mut bytes = [0; 4];
    process.read(address, &mut bytes).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), 8);
    let increment = resolve("kernel32", "InterlockedIncrement").unwrap();
    for invalid in [0, address + 1, 0xffff_fffc] {
        assert!(kernel.call(&mut process, increment.id, &[invalid]).is_err());
    }
    process.map(0x2000, &[0; 4], false).unwrap();
    let compare = resolve("kernel32", "InterlockedCompareExchange").unwrap();
    assert!(kernel
        .call(&mut process, compare.id, &[0x2000, 2, 1])
        .is_err());
}

#[test]
fn locale_wide_and_ansi_sizes_use_their_own_units_and_preserve_short_buffers() {
    let mut process = process();
    let mut kernel = Kernel::default();
    let output = process.heap_alloc(32).unwrap();
    for (name, width) in [("GetLocaleInfoA", 1), ("GetLocaleInfoW", 2)] {
        assert_eq!(call(&mut kernel, &mut process, name, &[0, 1, 0, 0]), 5);
        process.write(output, &[0xaa; 32]).unwrap();
        assert_eq!(call(&mut kernel, &mut process, name, &[0, 1, output, 4]), 0);
        assert_eq!(kernel.thread().error, 122);
        let mut unchanged = [0; 32];
        process.read(output, &mut unchanged).unwrap();
        assert_eq!(unchanged, [0xaa; 32]);
        assert_eq!(call(&mut kernel, &mut process, name, &[0, 1, output, 5]), 5);
        let mut bytes = vec![0; 5 * width];
        process.read(output, &mut bytes).unwrap();
        assert_eq!(
            bytes,
            if width == 1 {
                b"0409\0".to_vec()
            } else {
                vec![b'0', 0, b'4', 0, b'0', 0, b'9', 0, 0, 0]
            }
        );
        assert_eq!(
            call(&mut kernel, &mut process, name, &[0, 0x20000001, 0, 0]),
            4 / width as u32
        );
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                name,
                &[0, 0x20000001, output, 4 / width as u32]
            ),
            4 / width as u32
        );
        let mut number = [0; 4];
        process.read(output, &mut number).unwrap();
        assert_eq!(u32::from_le_bytes(number), 0x409);
    }
}
