use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};
#[test]
fn cursor_file_identity_and_destruction_preserve_working_backend_contract() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let load = resolve("user32", "LoadCursorFromFileA").unwrap();
    // No file decode or pointer read was performed by the source implementation.
    let handle = kernel.call(&mut process, load.id, &[0]).unwrap();
    assert_eq!(handle, 0x30010);
    assert_eq!(
        kernel.call(&mut process, load.id, &[0xffffffff]).unwrap(),
        handle
    );
    let set = resolve("user32", "SetCursor").unwrap();
    assert_eq!(kernel.call(&mut process, set.id, &[handle]).unwrap(), 0);
    let destroy = resolve("user32", "DestroyCursor").unwrap();
    assert_eq!(kernel.call(&mut process, destroy.id, &[handle]).unwrap(), 1);
    // Destruction was success-only; the current cursor slot is not implicitly cleared.
    assert_eq!(kernel.call(&mut process, set.id, &[0]).unwrap(), handle);
}
#[test]
fn compatibility_outputs_and_buffer_writes_match_source_without_claiming_new_services() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0xaa; 256], true).unwrap();
    for (name, args, value) in [
        ("BeginPaint", vec![0, 0x20000], 0x20000),
        ("EndPaint", vec![0, 0x20000], 1),
        ("FillRect", vec![0, 0, 0], 1),
        ("CreateMenu", vec![], 0),
        ("CreatePopupMenu", vec![], 0),
        ("AppendMenuA", vec![0; 4], 1),
        ("DestroyMenu", vec![0], 1),
        ("TrackPopupMenu", vec![0; 6], 0),
        ("GetMenu", vec![0], 0),
        ("MoveWindow", vec![0; 6], 1),
        ("SetForegroundWindow", vec![0], 1),
        ("GetParent", vec![0], 0),
        ("MsgWaitForMultipleObjects", vec![0; 5], 258),
        ("OpenClipboard", vec![0], 1),
        ("CloseClipboard", vec![], 1),
        ("EmptyClipboard", vec![], 1),
        ("SetClipboardData", vec![0; 2], 0),
        ("GetClipboardData", vec![0], 0),
        ("MessageBoxA", vec![0; 4], 1),
        ("MessageBoxW", vec![0; 4], 1),
    ] {
        assert_eq!(call(&mut process, name, &args).unwrap(), value, "{name}");
        let api = resolve("user32", name).unwrap();
        assert_eq!(api.arity, args.len() as u32);
    }
    let mut before = [0; 256];
    process.read(0x20000, &mut before).unwrap();
    assert_eq!(before, [0xaa; 256]);
    assert_eq!(
        call(
            &mut process,
            "SystemParametersInfoA",
            &[0x30, 0, 0x20000, 0]
        )
        .unwrap(),
        1
    );
    assert_eq!(words::<_, 4>(&process, 0x20000).unwrap(), [0, 0, 800, 600]);
    assert_eq!(
        call(&mut process, "SystemParametersInfoA", &[0xffff, 0, 0, 0]).unwrap(),
        1
    );
    assert_eq!(
        call(&mut process, "ChangeDisplaySettingsA", &[0x20000, 0]).unwrap(),
        0
    );
    let mut extra = [0; 2];
    process.read(0x20026, &mut extra).unwrap();
    assert_eq!(extra, [0; 2]);
    assert!(call(&mut process, "ChangeDisplaySettingsA", &[u32::MAX, 0]).is_err());
}
#[test]
fn wvsprintf_uses_explicit_guest_argument_list_and_fixed_stdcall_arity() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 1024], true).unwrap();
    process.write(0x20100, b"%s:%u\0").unwrap();
    process.write(0x20200, b"port\0").unwrap();
    write_words(&mut process, 0x20300, &[0x20200, 42]).unwrap();
    let api = resolve("user32", "wvsprintfA").unwrap();
    assert_eq!((api.arity, api.stack_args), (3, 3));
    assert!(!api.variadic);
    let mut kernel = Kernel::default();
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0x20000, 0x20100, 0x20300])
            .unwrap(),
        7
    );
    assert_eq!(process.read_c_string(0x20000, 32).unwrap(), b"port:42");
}

#[test]
fn every_old_user32_export_has_its_preserved_argument_count() {
    // Inventory of working src/runtime/{wasm/exports,wasm/compat-exports,winapi/user32}.ts.
    let contracts = [
        ("AdjustWindowRect", 3),
        ("AdjustWindowRectEx", 4),
        ("AppendMenuA", 4),
        ("BeginPaint", 2),
        ("ChangeDisplaySettingsA", 2),
        ("ClientToScreen", 2),
        ("CloseClipboard", 0),
        ("CreateMenu", 0),
        ("CreatePopupMenu", 0),
        ("CreateWindowExA", 12),
        ("DefWindowProcA", 4),
        ("DestroyCursor", 1),
        ("DestroyMenu", 1),
        ("DestroyWindow", 1),
        ("DispatchMessageA", 1),
        ("DrawTextA", 5),
        ("EmptyClipboard", 0),
        ("EndPaint", 2),
        // Old TS arity/ret used4; Win32 and the current call sites use3.
        ("EnumDisplaySettingsA", 3),
        ("FillRect", 3),
        ("FindWindowExA", 4),
        ("GetAsyncKeyState", 1),
        ("GetCapture", 0),
        ("GetClassInfoA", 3),
        ("GetClientRect", 2),
        ("GetClipboardData", 1),
        ("GetCursorPos", 1),
        ("GetDC", 1),
        ("GetKeyState", 1),
        ("GetKeyboardState", 1),
        ("GetMenu", 1),
        ("GetMessageA", 4),
        ("GetParent", 1),
        ("GetSystemMetrics", 1),
        ("GetWindowLongA", 2),
        ("GetWindowRect", 2),
        ("InflateRect", 3),
        ("IntersectRect", 3),
        ("InvalidateRect", 3),
        ("IsRectEmpty", 1),
        ("IsWindowVisible", 1),
        ("LoadCursorA", 2),
        ("LoadCursorFromFileA", 1),
        ("LoadIconA", 2),
        ("MessageBoxA", 4),
        ("MessageBoxW", 4),
        ("MoveWindow", 6),
        ("MsgWaitForMultipleObjects", 5),
        ("OffsetRect", 3),
        ("OpenClipboard", 1),
        ("PeekMessageA", 5),
        ("PostMessageA", 4),
        ("PostQuitMessage", 1),
        ("PtInRect", 3),
        ("RegisterClassA", 1),
        ("RegisterClassExA", 1),
        ("ReleaseCapture", 0),
        ("ReleaseDC", 2),
        ("ScreenToClient", 2),
        ("SetCapture", 1),
        ("SetClipboardData", 2),
        ("SetCursor", 1),
        ("SetCursorPos", 2),
        ("SetFocus", 1),
        ("SetForegroundWindow", 1),
        ("SetRect", 5),
        ("SetWindowLongA", 3),
        ("SetWindowPos", 7),
        ("SetWindowTextA", 2),
        ("ShowCursor", 1),
        ("ShowWindow", 2),
        ("SystemParametersInfoA", 4),
        ("TrackPopupMenu", 6),
        ("TranslateMessage", 1),
        ("UnionRect", 3),
        ("UpdateWindow", 1),
        ("wsprintfA", 2),
        ("wvsprintfA", 3),
    ];
    for (name, arity) in contracts {
        let api = resolve("user32", name)
            .unwrap_or_else(|| panic!("missing migrated USER32 export {name}"));
        assert_eq!(api.arity, arity, "{name}");
    }
}
