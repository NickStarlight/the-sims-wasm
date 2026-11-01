use super::*;
use crate::SparseMemory;

fn setup() -> (User, Process<SparseMemory>) {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Window\0").unwrap();
    write_words(
        &mut process,
        0x20000,
        &[0, 0x401000, 0, 9, 0x400000, 0, 0, 0, 0, 0x20100],
    )
    .unwrap();
    let mut user = User::default();
    user.register(&mut process, 0x20000, false).unwrap();
    (user, process)
}
fn create(user: &mut User, process: &mut Process<SparseMemory>) -> u32 {
    let Action::Invoke { args, next, .. } = user
        .begin(
            process,
            "CreateWindowExA",
            &[0, 1, 0, 0x90000000, 0, 0, 800, 600, 0, 0, 0x400000, 0],
        )
        .unwrap()
    else {
        panic!()
    };
    let hwnd = args[0];
    let Action::Invoke { next, .. } = user.resume(next, 1).unwrap() else {
        panic!()
    };
    assert!(matches!(user.resume(next,0).unwrap(),Action::Return(value) if value==hwnd));
    hwnd
}
#[test]
fn extra_bytes_are_zero_initialized_per_window_and_checked_as_byte_offsets() {
    let (mut user, mut process) = setup();
    let first = create(&mut user, &mut process);
    let second = create(&mut user, &mut process);
    assert_eq!(user.window_long(first, 0, None), 0);
    assert_eq!(user.window_long(first, 0, Some(0x100a4a0)), 0);
    assert_eq!(user.window_long(first, 0, None), 0x100a4a0);
    assert_eq!(user.window_long(second, 0, None), 0);
    assert_eq!(user.window_long(first, 5, Some(0xabcdef01)), 0);
    assert_eq!(user.window_long(first, 5, Some(0x76543210)), 0xabcdef01);
    for index in [6, 9, i32::MAX, -999] {
        assert_eq!(user.window_long(first, index, Some(1)), 0);
        assert_eq!(user.take_error(), Some(1413));
    }
    assert_eq!(user.window_long(first, 5, None), 0x76543210);
    assert_eq!(user.window_long(0, 0, None), 0);
    assert_eq!(user.take_error(), Some(1400));
}
#[test]
fn subclassed_dispatch_uses_new_proc_and_call_window_proc_reaches_previous_proc() {
    let (mut user, mut process) = setup();
    let hwnd = create(&mut user, &mut process);
    let old = user.window_long(hwnd, -4, Some(0x402000));
    assert_eq!(old, 0x401000);
    let Action::Invoke {
        address,
        args,
        next,
    } = user
        .begin(&mut process, "SendMessageA", &[hwnd, 0x400, 12, 34])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(address, 0x402000);
    assert_eq!(args, [hwnd, 0x400, 12, 34]);
    let Action::Invoke {
        address,
        args,
        next: parent,
    } = user
        .begin(&mut process, "CallWindowProcA", &[old, hwnd, 0x400, 12, 34])
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(address, old);
    assert_eq!(args, [hwnd, 0x400, 12, 34]);
    assert!(matches!(
        user.resume(parent, 77).unwrap(),
        Action::Return(77)
    ));
    assert!(matches!(user.resume(next, 88).unwrap(), Action::Return(88)));
    assert_eq!(user.window_long(hwnd, -4, Some(0)), 0);
    assert_eq!(user.take_error(), Some(87));
    assert_eq!(user.window_long(hwnd, -4, None), 0x402000);
}
#[test]
fn negative_fields_return_previous_values_and_reject_unsupported_geometry_mutations() {
    let (mut user, mut process) = setup();
    let hwnd = create(&mut user, &mut process);
    assert_eq!(user.window_long(hwnd, -21, Some(0xf000abcd)), 0);
    assert_eq!(user.window_long(hwnd, -21, None), 0xf000abcd);
    assert_eq!(user.window_long(hwnd, -6, Some(0x500000)), 0x400000);
    assert_eq!(user.window_long(hwnd, -16, Some(0x80000000)), 0x90000000);
    assert!(!user.windows[&hwnd].visible);
    user.show(hwnd, 1).unwrap();
    assert_eq!(user.window_long(hwnd, -16, None), 0x90000000);
    assert_eq!(user.window_long(hwnd, -16, Some(0x90c00000)), 0);
    assert_eq!(user.take_error(), Some(120));
    assert_eq!(user.window_long(hwnd, -16, None), 0x90000000);
    assert_eq!(user.window_long(hwnd, -20, Some(8)), 0);
    assert_eq!(user.window_long(hwnd, -20, None), 8);
    assert_eq!(user.window_long(hwnd, -20, Some(0x200)), 0);
    assert_eq!(user.take_error(), Some(120));
    assert_eq!(user.window_long(hwnd, -20, None), 8);
}
