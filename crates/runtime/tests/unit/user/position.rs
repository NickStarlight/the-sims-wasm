use super::*;
use crate::SparseMemory;
fn setup() -> (User, Process<SparseMemory>, u32) {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Window\0").unwrap();
    write_words(
        &mut process,
        0x20000,
        &[0, 0x401000, 0, 4, 0x400000, 0, 0, 0, 0, 0x20100],
    )
    .unwrap();
    let mut user = User::default();
    user.register(&mut process, 0x20000, false).unwrap();
    let Action::Invoke { args, next, .. } = user
        .begin(
            &mut process,
            "CreateWindowExA",
            &[0, 1, 0, 0x90000000, 10, 20, 800, 600, 0, 0, 0x400000, 0],
        )
        .unwrap()
    else {
        panic!()
    };
    let hwnd = args[0];
    let Action::Invoke { next, .. } = user.resume(next, 1).unwrap() else {
        panic!()
    };
    user.resume(next, 0).unwrap();
    (user, process, hwnd)
}
fn callback(action: Action, message: u32) -> (u32, Continuation) {
    let Action::Invoke { args, next, .. } = action else {
        panic!("expected callback")
    };
    assert_eq!(args[1], message);
    (args[3], next)
}
fn position(user: &User, hwnd: u32) -> [u32; 4] {
    let w = &user.windows[&hwnd];
    [w.x, w.y, w.width, w.height]
}
#[test]
fn actual_frame_change_flags_ignore_dimensions_but_send_nonclient_and_changed_callbacks() {
    let (mut user, mut process, hwnd) = setup();
    let (address, next) = callback(
        user.begin(
            &mut process,
            "SetWindowPos",
            &[hwnd, 0, 0, 0, u32::MAX, u32::MAX, 0x37],
        )
        .unwrap(),
        0x46,
    );
    let (params, next) = callback(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        0x83,
    );
    assert_eq!(
        words::<_, 13>(&process, params).unwrap(),
        [10, 20, 810, 620, 10, 20, 810, 620, 10, 20, 810, 620, address]
    );
    let (changed, next) = callback(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        0x47,
    );
    assert_eq!(changed, address);
    assert_eq!(position(&user, hwnd), [10, 20, 800, 600]);
    assert!(matches!(
        user.begin(&mut process, "DefWindowProcA", &[hwnd, 0x47, 0, address])
            .unwrap(),
        Action::Return(0)
    ));
    assert!(matches!(
        user.resume_with_memory(&mut process, next, 123).unwrap(),
        Action::Return(1)
    ));
    assert!(user.positions.active.is_empty());
}
#[test]
fn callback_edits_apply_and_default_changed_handler_emits_move_and_size() {
    let (mut user, mut process, hwnd) = setup();
    let (address, next) = callback(
        user.begin(
            &mut process,
            "SetWindowPos",
            &[hwnd, 0, 30, 40, 640, 480, 0x14],
        )
        .unwrap(),
        0x46,
    );
    write_words(&mut process, address + 8, &[50, 60, 1024, 768]).unwrap();
    let (_, next) = callback(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        0x83,
    );
    let (_, next) = callback(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        0x47,
    );
    assert_eq!(position(&user, hwnd), [50, 60, 1024, 768]);
    let (point, notification) = callback(
        user.begin(&mut process, "DefWindowProcA", &[hwnd, 0x47, 0, address])
            .unwrap(),
        3,
    );
    assert_eq!(point, 50 | (60 << 16));
    let (size, notification) = callback(user.resume(notification, 0).unwrap(), 5);
    assert_eq!(size, 1024 | (768 << 16));
    assert!(matches!(
        user.resume(notification, 0).unwrap(),
        Action::Return(0)
    ));
    // Changes made during WINDOWPOSCHANGED do not change committed state.
    write_words(&mut process, address + 8, &[99, 99, 1, 1]).unwrap();
    user.resume_with_memory(&mut process, next, 0).unwrap();
    assert_eq!(position(&user, hwnd), [50, 60, 1024, 768]);
}
#[test]
fn nested_transactions_have_distinct_scratch_and_cancel_does_not_destroy_window() {
    let (mut user, mut process, hwnd) = setup();
    let args = [hwnd, 0, 0, 0, 800, 600, 0x37];
    let (outer, first) = callback(
        user.begin(&mut process, "SetWindowPos", &args).unwrap(),
        0x46,
    );
    let (inner, second) = callback(
        user.begin(&mut process, "SetWindowPos", &args).unwrap(),
        0x46,
    );
    assert_ne!(outer, inner);
    user.cancel(second);
    assert!(user.windows.contains_key(&hwnd));
    assert_eq!(user.positions.active.len(), 1);
    user.cancel(first);
    assert!(user.positions.active.is_empty());
    assert_eq!(user.positions.free.len(), 2);
}
#[test]
fn invalid_handles_mutated_structures_and_bounds_fail_without_committing() {
    let (mut user, mut process, hwnd) = setup();
    assert!(matches!(
        user.begin(&mut process, "SetWindowPos", &[0, 0, 0, 0, 1, 1, 0x37])
            .unwrap(),
        Action::Return(0)
    ));
    assert_eq!(user.take_error(), Some(1400));
    assert!(matches!(
        user.begin(&mut process, "SetWindowPos", &[hwnd, 0, 0, 0, 1, 1, 0])
            .unwrap(),
        Action::Return(0)
    ));
    assert_eq!(user.take_error(), Some(120));
    let (address, next) = callback(
        user.begin(
            &mut process,
            "SetWindowPos",
            &[hwnd, 0, 0, 0, 800, 600, 0x14],
        )
        .unwrap(),
        0x46,
    );
    write_words(&mut process, address, &[0]).unwrap();
    assert!(matches!(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        Action::Return(0)
    ));
    assert_eq!(user.take_error(), Some(87));
    let (_, next) = callback(
        user.begin(
            &mut process,
            "SetWindowPos",
            &[hwnd, 0, i32::MAX as u32, 0, 2, 1, 0x14],
        )
        .unwrap(),
        0x46,
    );
    assert!(user.resume_with_memory(&mut process, next, 0).is_err());
    assert!(user.positions.active.is_empty());
    assert_eq!(position(&user, hwnd), [10, 20, 800, 600]);
    let (params, next) = callback(
        user.begin(
            &mut process,
            "SetWindowPos",
            &[hwnd, 0, 0, 0, 800, 600, 0x414],
        )
        .unwrap(),
        0x83,
    );
    write_words(&mut process, params, &[1]).unwrap();
    assert!(matches!(
        user.resume_with_memory(&mut process, next, 0).unwrap(),
        Action::Return(0)
    ));
    assert_eq!(user.take_error(), Some(120));
    assert_eq!(position(&user, hwnd), [10, 20, 800, 600]);
    assert!(user.positions.active.is_empty());
}
