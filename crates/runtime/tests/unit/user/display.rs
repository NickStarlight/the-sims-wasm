use super::*;
use crate::{
    kernel::{manifest, resolve, Kernel},
    SparseMemory,
};
fn setup() -> (Kernel, Process<SparseMemory>, u32) {
    let mut kernel = Kernel::default();
    let bindings: Vec<_> = manifest()
        .iter()
        .filter(|api| api.dll == "ddraw.dll")
        .map(|api| serde_json::json!({"dll":api.dll,"name":api.name,"address":0x800000+api.id*4}))
        .collect();
    kernel.install_modules(&serde_json::json!({"images":[{"base":0x400000,"sizeOfImage":4096,"isDll":false}],"nativeDlls":["ddraw.dll"],"nativeBindings":bindings}).to_string()).unwrap();
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let buffer = process.heap_alloc(256).unwrap();
    (kernel, process, buffer)
}
fn call(k: &mut Kernel, p: &mut Process<SparseMemory>, dll: &str, name: &str, args: &[u32]) -> u32 {
    k.call(p, resolve(dll, name).unwrap().id, args).unwrap()
}
fn prepare(p: &mut Process<SparseMemory>, buffer: u32, size: u16) {
    p.write(buffer, &[0xa5; 256]).unwrap();
    p.write(buffer + 36, &size.to_le_bytes()).unwrap();
}
fn output(p: &Process<SparseMemory>, buffer: u32) -> [u32; 5] {
    words(p, buffer + 104).unwrap()
}
#[test]
fn enum_modes_write_legacy_and_modern_layouts_and_stop_at_end() {
    let (mut k, mut p, b) = setup();
    for (index, &(width, height, bits)) in crate::display::MODES.iter().enumerate() {
        prepare(&mut p, b, 124);
        assert_eq!(
            call(
                &mut k,
                &mut p,
                "user32.dll",
                "EnumDisplaySettingsA",
                &[0, index as u32, b]
            ),
            1
        );
        assert_eq!(output(&p, b), [bits, width, height, 0, 60]);
        let mut tail = [0; 4];
        p.read(b + 124, &mut tail).unwrap();
        assert_eq!(tail, [0xa5; 4]);
        assert_eq!(words::<_, 1>(&p, b + 40).unwrap(), [0x7c0000]);
    }
    prepare(&mut p, b, 156);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[0, 3, b]
        ),
        0
    );
    let mut byte = [0];
    p.read(b, &mut byte).unwrap();
    assert_eq!(byte, [0xa5]);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[0, u32::MAX, b]
        ),
        1
    );
    assert_eq!(output(&p, b), [16, 800, 600, 0, 60]);
    p.read(b + 156, &mut byte).unwrap();
    assert_eq!(byte, [0xa5]);
}
#[test]
fn directdraw_changes_current_metrics_but_not_registry_default_or_other_process() {
    let (mut k, mut p, b) = setup();
    assert_eq!(
        call(&mut k, &mut p, "ddraw.dll", "DirectDrawCreate", &[0, b, 0]),
        0
    );
    let device = words::<_, 1>(&p, b).unwrap()[0];
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "ddraw.dll",
            "DDraw_SetDisplayMode",
            &[device, 1024, 768, 32]
        ),
        0
    );
    assert_eq!(
        call(&mut k, &mut p, "user32.dll", "GetSystemMetrics", &[0]),
        1024
    );
    assert_eq!(
        call(&mut k, &mut p, "user32.dll", "GetSystemMetrics", &[1]),
        768
    );
    prepare(&mut p, b, 156);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[0, u32::MAX, b]
        ),
        1
    );
    assert_eq!(output(&p, b), [32, 1024, 768, 0, 60]);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[0, 0xfffffffe, b]
        ),
        1
    );
    assert_eq!(output(&p, b), [16, 800, 600, 0, 60]);
    let mut other = Kernel::default();
    assert_eq!(
        call(&mut other, &mut p, "user32.dll", "GetSystemMetrics", &[0]),
        800
    );
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "ddraw.dll",
            "DDraw_RestoreDisplayMode",
            &[device]
        ),
        0
    );
    assert_eq!(
        call(&mut k, &mut p, "user32.dll", "GetSystemMetrics", &[0]),
        800
    );
}
#[test]
fn invalid_names_sizes_and_guest_buffers_do_not_corrupt_records() {
    let (mut k, mut p, b) = setup();
    prepare(&mut p, b, 120);
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[0, 0, b]
        ),
        0
    );
    assert_eq!(
        call(&mut k, &mut p, "kernel32.dll", "GetLastError", &[]),
        87
    );
    p.write(b + 200, b"\\\\.\\DISPLAY2\0").unwrap();
    prepare(&mut p, b, 156);
    p.write(b + 200, b"\\\\.\\DISPLAY2\0").unwrap();
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[b + 200, 0, b]
        ),
        0
    );
    p.write(b + 200, b"\\\\.\\display1\0").unwrap();
    assert_eq!(
        call(
            &mut k,
            &mut p,
            "user32.dll",
            "EnumDisplaySettingsA",
            &[b + 200, 0, b]
        ),
        1
    );
    let id = resolve("user32.dll", "EnumDisplaySettingsA").unwrap().id;
    assert!(k.call(&mut p, id, &[0, 0, 0xfffffff0]).is_err());
}
