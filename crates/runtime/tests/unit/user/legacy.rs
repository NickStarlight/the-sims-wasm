use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};
fn setup() -> (User, Process<SparseMemory>, u32) {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Window\0").unwrap();
    process.write(0x20200, b"Original\0").unwrap();
    write_words(
        &mut process,
        0x20000,
        &[0, 0x401000, 0, 4, 0x400000, 0, 0, 0, 0, 0x20100],
    )
    .unwrap();
    let mut user = User::default();
    user.register(&mut process, 0x20000, false).unwrap();
    let hwnd = create(&mut user, &mut process);
    (user, process, hwnd)
}
fn create(user: &mut User, process: &mut Process<SparseMemory>) -> u32 {
    let Action::Invoke { args, next, .. } = user
        .begin(
            process,
            "CreateWindowExA",
            &[0, 1, 0x20200, 0x90000000, 0, 0, 800, 600, 0, 0, 0x400000, 0],
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
    hwnd
}
#[test]
fn window_text_preserves_success_only_contract_without_title_mutation_or_callbacks() {
    let (mut user, mut process, hwnd) = setup();
    process.write(0x20300, b"Changed\0").unwrap();
    assert_eq!(
        user.call(&mut process, "SetWindowTextA", &[hwnd, 0x20300])
            .unwrap(),
        1
    );
    assert_eq!(user.windows[&hwnd].title, b"Original");
    assert!(user.messages.is_empty());
    assert_eq!(
        user.call(&mut process, "SetWindowTextA", &[0, 0]).unwrap(),
        1
    );
    assert!(!resolve("user32", "SetWindowTextA").unwrap().callback);
}
#[test]
fn dirty_paint_precedes_periodic_timer_and_empty_getmessage_preserves_old_fallback() {
    let (mut user, mut process, hwnd) = setup();
    let peek = [0x20400, 0, 0, 0, 1];
    user.post(Message::new(hwnd, 0x400, 0, 0)).unwrap();
    user.peek(&mut process, &peek).unwrap();
    assert_eq!(Message::read(&process, peek[0]).unwrap().id, 0x400);
    user.peek(&mut process, &peek).unwrap();
    assert_eq!(Message::read(&process, peek[0]).unwrap().id, 0xf);
    user.set_time(14);
    assert_eq!(user.peek(&mut process, &peek).unwrap(), 0);
    assert_eq!(
        user.call(&mut process, "GetMessageA", &peek[..4]).unwrap(),
        1
    );
    assert_eq!(
        words::<_, 7>(&process, peek[0]).unwrap(),
        [0, 0, 0, 0, 14, 0, 0]
    );
    user.set_time(15);
    assert_eq!(user.peek(&mut process, &peek).unwrap(), 1);
    let msg = Message::read(&process, peek[0]).unwrap();
    assert_eq!((msg.id, msg.time, msg.hwnd), (0x113, 15, hwnd));
    assert_eq!(user.peek(&mut process, &peek).unwrap(), 0);
    user.call(&mut process, "InvalidateRect", &[hwnd, 0, 1])
        .unwrap();
    user.call(&mut process, "UpdateWindow", &[hwnd]).unwrap();
    assert_eq!(user.dirty.len(), 1);
    assert!(user.messages.is_empty());
    user.peek(&mut process, &peek).unwrap();
    assert_eq!(Message::read(&process, peek[0]).unwrap().id, 0xf);
    let mut kernel = Kernel::default();
    let api = resolve("user32", "PeekMessageA").unwrap();
    assert!(api.clock);
    assert!(kernel.call(&mut process, api.id, &peek).is_err());
    kernel.set_monotonic_time(15.0).unwrap();
    assert_eq!(kernel.call(&mut process, api.id, &peek).unwrap(), 1);
}
#[test]
fn capture_changes_callback_and_redirect_already_queued_mouse_only() {
    let (mut user, mut process, first) = setup();
    let second = create(&mut user, &mut process);
    user.set_focus(first);
    assert!(matches!(
        user.begin(&mut process, "SetCapture", &[first]).unwrap(),
        Action::Return(0)
    ));
    user.set_focus(0);
    user.mouse_input(8, 9, 0).unwrap();
    user.set_focus(first);
    user.post(Message::new(first, 0x201, 0, 0)).unwrap();
    let Action::Invoke { args, next, .. } =
        user.begin(&mut process, "SetCapture", &[second]).unwrap()
    else {
        panic!()
    };
    assert_eq!(args, [first, 0x215, 0, second]);
    assert!(matches!(user.resume(next,0).unwrap(),Action::Return(value) if value==first));
    user.peek(&mut process, &[0x20400, second, 0, 0, 1])
        .unwrap();
    assert_eq!(Message::read(&process, 0x20400).unwrap().hwnd, second);
    user.peek(&mut process, &[0x20400, first, 0, 0, 1]).unwrap();
    assert_eq!(Message::read(&process, 0x20400).unwrap().hwnd, first);
    let Action::Invoke { args, next, .. } =
        user.begin(&mut process, "ReleaseCapture", &[]).unwrap()
    else {
        panic!()
    };
    assert_eq!(args, [second, 0x215, 0, 0]);
    assert!(matches!(user.resume(next, 0).unwrap(), Action::Return(1)));
    assert_eq!(user.capture, 0);
}
#[test]
fn keyboard_snapshots_aggregate_modifier_sides_and_cursor_state_is_owned() {
    let (mut user, mut process, _) = setup();
    user.key_input(0xa0, true, false).unwrap();
    user.call(&mut process, "GetKeyboardState", &[0x20400])
        .unwrap();
    let mut keys = [0; 256];
    process.read(0x20400, &mut keys).unwrap();
    assert_eq!(keys[0x10], 0);
    user.peek(&mut process, &[0x20500, 0, 0, 0, 1]).unwrap();
    user.call(&mut process, "GetKeyboardState", &[0x20400])
        .unwrap();
    process.read(0x20400, &mut keys).unwrap();
    assert_eq!((keys[0x10], keys[0xa0]), (0x80, 0x80));
    user.key_input(0xa1, true, false).unwrap();
    user.key_input(0xa0, false, false).unwrap();
    assert_eq!(user.key_state(0x10, true).unwrap(), 0xffff8001);
    assert_eq!(
        user.call(&mut process, "ShowCursor", &[0]).unwrap(),
        u32::MAX
    );
    assert_eq!(user.call(&mut process, "ShowCursor", &[1]).unwrap(), 0);
    assert_eq!(user.call(&mut process, "SetCursor", &[123]).unwrap(), 0);
    assert_eq!(user.call(&mut process, "SetCursor", &[456]).unwrap(), 123);
    user.call(&mut process, "SetCursorPos", &[u32::MAX, 7])
        .unwrap();
    assert_eq!(user.input.cursor, [-1, 7]);
    assert_eq!(
        user.call(&mut process, "GetKeyboardState", &[0]).unwrap(),
        0
    );
}
#[test]
fn rectangle_aliases_edges_wrapping_and_fixed_font_measurement_match_old_contracts() {
    let (mut user, mut process, _) = setup();
    let rect = 0x20400;
    user.call(&mut process, "SetRect", &[rect, 0, 0, 10, 10])
        .unwrap();
    assert_eq!(
        user.call(&mut process, "PtInRect", &[rect, 9, 9]).unwrap(),
        1
    );
    assert_eq!(
        user.call(&mut process, "PtInRect", &[rect, 10, 9]).unwrap(),
        0
    );
    user.call(&mut process, "SetRect", &[rect + 16, 5, 5, 20, 20])
        .unwrap();
    assert_eq!(
        user.call(&mut process, "IntersectRect", &[rect, rect, rect + 16])
            .unwrap(),
        1
    );
    assert_eq!(words::<_, 4>(&process, rect).unwrap(), [5, 5, 10, 10]);
    user.call(&mut process, "OffsetRect", &[rect, u32::MAX, 2])
        .unwrap();
    assert_eq!(words::<_, 4>(&process, rect).unwrap(), [4, 7, 9, 12]);
    user.call(&mut process, "InflateRect", &[rect, 1, 2])
        .unwrap();
    assert_eq!(words::<_, 4>(&process, rect).unwrap(), [3, 5, 10, 14]);
    assert_eq!(user.call(&mut process, "IsRectEmpty", &[0]).unwrap(), 1);
    process.write(0x20300, b"aa\r\nbbb\nc\0").unwrap();
    write_words(&mut process, rect, &[5, 6, 0, 0]).unwrap();
    assert_eq!(
        user.call(
            &mut process,
            "DrawTextA",
            &[0, 0x20300, u32::MAX, rect, 0x400]
        )
        .unwrap(),
        48
    );
    assert_eq!(words::<_, 4>(&process, rect).unwrap(), [5, 6, 29, 54]);
}
