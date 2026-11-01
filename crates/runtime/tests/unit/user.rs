use super::*;
use crate::SparseMemory;
fn setup() -> (User, Process<SparseMemory>) {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Gonzo\0").unwrap();
    write_words(
        &mut process,
        0x20000,
        &[0, 0x401000, 0, 0, 0x400000, 0, 0, 0, 0, 0x20100],
    )
    .unwrap();
    (User::default(), process)
}
fn created(user: &mut User, process: &mut Process<SparseMemory>) -> u32 {
    assert_eq!(user.register(process, 0x20000, false).unwrap(), 1);
    let a = [
        0, 0x20100, 0, 0, 10, 20, 800, 600, 0, 0, 0x400000, 0xdeadbeef,
    ];
    let Action::Invoke { args, next, .. } = user.begin(process, "CreateWindowExA", &a).unwrap()
    else {
        panic!()
    };
    let hwnd = args[0];
    assert_eq!(&args[1..3], &[0x81, 0]);
    assert_eq!(
        words::<_, 12>(process, args[3]).unwrap(),
        [0xdeadbeef, 0x400000, 0, 0, 600, 800, 20, 10, 0, 0, 0x20100, 0]
    );
    let Action::Invoke { args, next, .. } = user.resume(next, 1).unwrap() else {
        panic!()
    };
    assert_eq!(args[1], 1);
    assert!(matches!(user.resume(next,0).unwrap(),Action::Return(h) if h==hwnd));
    hwnd
}
#[test]
fn class_layout_create_chain_and_borderless_geometry() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    assert_eq!(user.register(&mut process, 0x20000, false).unwrap(), 0);
    assert_eq!(user.take_error(), Some(1410));
    assert_eq!(
        user.default_proc(&mut process, &[hwnd, 0x81, 0, 0])
            .unwrap(),
        1
    );
    user.rect(&mut process, hwnd, 0x20200, false).unwrap();
    assert_eq!(
        words::<_, 4>(&process, 0x20200).unwrap(),
        [10, 20, 810, 620]
    );
    user.rect(&mut process, hwnd, 0x20200, true).unwrap();
    assert_eq!(words::<_, 4>(&process, 0x20200).unwrap(), [0, 0, 800, 600]);
    write_words(&mut process, 0x20200, &[2, 3]).unwrap();
    user.coordinate(&mut process, hwnd, 0x20200, true).unwrap();
    assert_eq!(words::<_, 2>(&process, 0x20200).unwrap(), [12, 23]);
}
#[test]
fn extended_class_atom_lookup_and_failed_creation_cleanup() {
    let (mut user, mut process) = setup();
    write_words(
        &mut process,
        0x20000,
        &[48, 0, 0x401000, 0, 0, 0x400000, 0, 0, 0, 0, 0x20100, 0],
    )
    .unwrap();
    assert_eq!(user.register(&mut process, 0x20000, true).unwrap(), 1);
    let Action::Invoke { next, .. } = user
        .begin(
            &mut process,
            "CreateWindowExA",
            &[0, 1, 0, 0, 0, 0, 800, 600, 0, 0, 0x400000, 0],
        )
        .unwrap()
    else {
        panic!()
    };
    let Action::Invoke { args, next, .. } = user.resume(next, 0).unwrap() else {
        panic!()
    };
    assert_eq!(args[1], 0x82);
    assert!(matches!(user.resume(next, 0).unwrap(), Action::Return(0)));
    assert!(user.windows.is_empty());
}
#[test]
fn queue_filters_peek_removal_quit_and_transactional_output() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    user.post(Message::new(hwnd, 0x200, 7, 8)).unwrap();
    user.post(Message::new(hwnd, 0x201, 9, 10)).unwrap();
    assert_eq!(
        user.peek(&mut process, &[0x20200, hwnd, 0x201, 0x201, 0])
            .unwrap(),
        1
    );
    assert_eq!(Message::read(&process, 0x20200).unwrap().wparam, 9);
    assert_eq!(user.messages.len(), 2);
    assert!(user.peek(&mut process, &[0, 0, 0, 0, 1]).is_err());
    assert_eq!(user.messages.len(), 2);
    assert_eq!(user.peek(&mut process, &[0x20200, 0, 0, 0, 1]).unwrap(), 1);
    assert_eq!(user.messages.len(), 1);
    user.enqueue(Message::new(0, 0x12, 42, 0)).unwrap();
    assert_eq!(
        user.peek(&mut process, &[0x20200, hwnd, 1, 1, 1]).unwrap(),
        1
    );
    assert_eq!(Message::read(&process, 0x20200).unwrap().id, 0x12);
}
#[test]
fn destroy_callbacks_and_cancellation_reclaim_window_slots() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    let Action::Invoke { args, next, .. } =
        user.begin(&mut process, "DestroyWindow", &[hwnd]).unwrap()
    else {
        panic!()
    };
    assert_eq!(args[1], 2);
    let Action::Invoke { args, next, .. } = user.resume(next, 99).unwrap() else {
        panic!()
    };
    assert_eq!(args[1], 0x82);
    user.cancel(next);
    assert!(user.windows.is_empty());
}

#[test]
fn kernel_window_callbacks_allow_reentry_and_cancel_without_stranded_handles() {
    use crate::{
        kernel::{resolve, Kernel},
        CallOutcome,
    };
    let (_, mut process) = setup();
    let mut kernel = Kernel::default();
    let register = resolve("user32", "RegisterClassA").unwrap();
    assert_eq!(
        kernel.call(&mut process, register.id, &[0x20000]).unwrap(),
        1
    );
    let create = resolve("user32", "CreateWindowExA").unwrap();
    let args = [0, 1, 0, 0, 0, 0, 800, 600, 0, 0, 0x400000, 0];
    let CallOutcome::Callback {
        args: callback_args,
        token,
        ..
    } = kernel.begin_call(&mut process, create.id, &args).unwrap()
    else {
        panic!()
    };
    let hwnd = callback_args[0];
    let default = resolve("user32", "DefWindowProcA").unwrap();
    let CallOutcome::Return { value: result } = kernel
        .begin_call(&mut process, default.id, &callback_args)
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(result, 1);
    let CallOutcome::Callback { token, .. } =
        kernel.resume_call(&mut process, token, result).unwrap()
    else {
        panic!()
    };
    let is_window = resolve("user32", "IsWindow").unwrap();
    assert_eq!(kernel.call(&mut process, is_window.id, &[hwnd]).unwrap(), 1);
    assert!(kernel.cancel_call(token));
    assert!(!kernel.cancel_call(token));
    assert_eq!(kernel.call(&mut process, is_window.id, &[hwnd]).unwrap(), 0);
    assert!(kernel.resume_call(&mut process, token, 0).is_err());
}

#[test]
fn input_has_no_phantom_window_and_preserves_mouse_and_key_transitions() {
    let (mut user, mut process) = setup();
    user.mouse_input(3, 4, 0).unwrap();
    user.key_input(0x41, true, false).unwrap();
    assert!(user.messages.is_empty());
    assert_eq!(user.key_state(0x41, false).unwrap(), 0);
    user.key_input(0x41, false, false).unwrap();
    let hwnd = created(&mut user, &mut process);
    user.set_focus(hwnd);
    user.mouse_input(-2, 5, 1).unwrap();
    let movement = user.messages.pop_front().unwrap();
    assert_eq!(movement.id, 0x200);
    assert_eq!(movement.lparam, 0x0005fffe);
    let click = user.messages.pop_front().unwrap();
    assert_eq!(click.id, 0x201);
    assert_eq!(click.wparam, 1);
    assert_eq!(click.hwnd, hwnd);
    assert_eq!(user.input.cursor, [8, 25]);
    assert_eq!(user.key_state(1, true).unwrap(), 0xffff8001);
    assert_eq!(user.key_state(1, true).unwrap(), 0xffff8000);
    user.key_input(0x41, true, false).unwrap();
    user.key_input(0x41, true, true).unwrap();
    user.key_input(0x41, false, false).unwrap();
    let down = user.messages.pop_front().unwrap();
    let repeat = user.messages.pop_front().unwrap();
    let up = user.messages.pop_front().unwrap();
    assert_eq!(
        (down.id, down.wparam, down.lparam),
        (0x100, 0x41, 0x001e0001)
    );
    assert_eq!(repeat.lparam, 0x401e0001);
    assert_eq!((up.id, up.lparam), (0x101, 0xc01e0001));
    assert_eq!(user.key_state(0x41, true).unwrap(), 1);
    assert_eq!(user.key_state(0x41, true).unwrap(), 0);
    user.key_input(0x14, true, false).unwrap();
    user.key_input(0x14, false, false).unwrap();
    assert_eq!(user.key_state(0x14, false).unwrap(), 0);
    user.peek(&mut process, &[0x20200, 0, 0, 0, 1]).unwrap();
    assert_eq!(user.key_state(0x14, false).unwrap(), 0xffff8001);
    user.peek(&mut process, &[0x20200, 0, 0, 0, 1]).unwrap();
    assert_eq!(user.key_state(0x14, false).unwrap(), 1);
    assert!(user.key_input(256, true, false).is_err());
    assert!(user.mouse_input(0, 0, 8).is_err());
}

#[test]
fn mouse_moves_coalesce_and_destroyed_focus_never_receives_input() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    user.set_focus(hwnd);
    user.mouse_input(1, 2, 0).unwrap();
    user.mouse_input(3, 4, 0).unwrap();
    assert_eq!(user.messages.len(), 1);
    assert_eq!(user.messages[0].lparam, 0x00040003);
    let Action::Invoke { next, .. } = user.begin(&mut process, "DestroyWindow", &[hwnd]).unwrap()
    else {
        panic!()
    };
    user.cancel(next);
    user.key_input(0x42, true, false).unwrap();
    assert!(user.messages.is_empty());
    assert_eq!(user.focus, None);
}

#[test]
fn rejected_create_runs_destroy_chain_and_returns_null() {
    let (mut user, mut process) = setup();
    user.register(&mut process, 0x20000, false).unwrap();
    let Action::Invoke { next, .. } = user
        .begin(
            &mut process,
            "CreateWindowExA",
            &[0, 1, 0, 0, 0, 0, 800, 600, 0, 0, 0x400000, 0],
        )
        .unwrap()
    else {
        panic!()
    };
    let Action::Invoke { next, .. } = user.resume(next, 1).unwrap() else {
        panic!()
    };
    let Action::Invoke { args, next, .. } = user.resume(next, u32::MAX).unwrap() else {
        panic!()
    };
    assert_eq!(args[1], 2);
    let Action::Invoke { args, next, .. } = user.resume(next, 0).unwrap() else {
        panic!()
    };
    assert_eq!(args[1], 0x82);
    assert!(matches!(user.resume(next, 0).unwrap(), Action::Return(0)));
    assert!(user.windows.is_empty());
}

#[test]
fn find_window_matches_existing_classes_titles_parent_and_start_after() {
    let (mut user, mut process) = setup();
    let first = created(&mut user, &mut process);
    process.write(0x20300, b"My Title\0").unwrap();
    let mut create = |user: &mut User, parent, style| {
        let Action::Invoke { next, .. } = user
            .begin(
                &mut process,
                "CreateWindowExA",
                &[0, 1, 0x20300, style, 0, 0, 10, 10, parent, 0, 0x400000, 0],
            )
            .unwrap()
        else {
            panic!()
        };
        let Action::Invoke { next, .. } = user.resume(next, 1).unwrap() else {
            panic!()
        };
        let Action::Return(hwnd) = user.resume(next, 0).unwrap() else {
            panic!()
        };
        hwnd
    };
    let second = create(&mut user, 0, 0);
    let child = create(&mut user, first, 0x40000000);
    let owned = create(&mut user, first, 0);
    process.write(0x20400, b"mY tITLE\0").unwrap();
    assert_eq!(user.find_window(&process, &[0, 0, 1, 0]).unwrap(), owned);
    assert_eq!(
        user.find_window(&process, &[0, owned, 0x20100, 0x20400])
            .unwrap(),
        second
    );
    assert_eq!(
        user.find_window(&process, &[0, second, 1, 0]).unwrap(),
        first
    );
    assert_eq!(
        user.find_window(&process, &[first, 0, 1, 0x20400]).unwrap(),
        child
    );
    assert_eq!(
        user.find_window(&process, &[first, child, 1, 0]).unwrap(),
        0
    );
    assert_eq!(user.find_window(&process, &[0, child, 1, 0]).unwrap(), 0);
    assert_eq!(user.take_error(), Some(1400));
    assert_eq!(user.find_window(&process, &[0, 0, 99, 0]).unwrap(), 0);
    user.cancel(Continuation::Create { hwnd: owned });
    assert_eq!(
        user.find_window(&process, &[0, 0, 1, 0x20400]).unwrap(),
        second
    );
}

#[test]
fn display_dc_leases_validate_windows_threads_and_release_exactly_once() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    assert_eq!(user.dc_call("GetDC", &[0xdeadbeef], 1).unwrap(), 0);
    assert_eq!(user.take_error(), Some(1400));
    let screen = user.dc_call("GetDC", &[0], 1).unwrap();
    let window = user.dc_call("GetDC", &[hwnd], 1).unwrap();
    assert_ne!(screen, window);
    assert_eq!(user.dc_call("ReleaseDC", &[0, window], 1).unwrap(), 0);
    assert_eq!(user.dc_call("ReleaseDC", &[hwnd, window], 2).unwrap(), 0);
    assert_eq!(user.dc_call("GetDeviceCaps", &[window, 8], 1).unwrap(), 800);
    assert_eq!(user.dc_call("ReleaseDC", &[hwnd, window], 1).unwrap(), 1);
    assert_eq!(user.dc_call("ReleaseDC", &[hwnd, window], 1).unwrap(), 0);
    assert_eq!(user.dc_call("GetDeviceCaps", &[window, 8], 1).unwrap(), 0);
    assert_eq!(user.take_error(), Some(6));
    let next = user.dc_call("GetDC", &[hwnd], 1).unwrap();
    assert_ne!(next, window);
    assert_eq!(user.dc_call("ReleaseDC", &[0, screen], 1).unwrap(), 1);
}

#[test]
fn destroy_or_cancel_reclaims_window_dcs_without_releasing_screen_dc() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    let screen = user.dc_call("GetDC", &[0], 1).unwrap();
    let dc = user.dc_call("GetDC", &[hwnd], 1).unwrap();
    let Action::Invoke { next, .. } = user.begin(&mut process, "DestroyWindow", &[hwnd]).unwrap()
    else {
        panic!()
    };
    user.cancel(next);
    assert_eq!(user.dc_call("GetDeviceCaps", &[dc, 8], 1).unwrap(), 0);
    assert_eq!(user.dc_call("ReleaseDC", &[hwnd, dc], 1).unwrap(), 0);
    assert_eq!(user.dc_call("GetDeviceCaps", &[screen, 8], 1).unwrap(), 800);
}

#[test]
fn device_caps_follow_shared_virtual_mode_and_process_handle_ownership() {
    use crate::kernel::{resolve, Kernel};
    let display = crate::display::Display::default();
    let mut user = User::with_display(display.clone());
    let dc = user.dc_call("GetDC", &[0], 1).unwrap();
    display.set_mode((1024, 768, 32));
    for (index, value) in [
        (8, 1024),
        (10, 768),
        (12, 32),
        (14, 1),
        (24, u32::MAX),
        (88, 96),
        (108, 24),
        (116, 60),
        (117, 768),
        (118, 1024),
    ] {
        assert_eq!(
            user.dc_call("GetDeviceCaps", &[dc, index], 1).unwrap(),
            value
        );
    }
    let mut kernel = Kernel::default();
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let caps = resolve("gdi32.dll", "GetDeviceCaps").unwrap();
    assert_eq!(kernel.call(&mut process, caps.id, &[dc, 8]).unwrap(), 0);
    let get = resolve("user32.dll", "GetDC").unwrap();
    let handle = kernel.call(&mut process, get.id, &[0]).unwrap();
    assert_eq!(
        kernel.call(&mut process, caps.id, &[handle, 8]).unwrap(),
        800
    );
    kernel.set_thread(2).unwrap();
    let release = resolve("user32.dll", "ReleaseDC").unwrap();
    assert_eq!(
        kernel.call(&mut process, release.id, &[0, handle]).unwrap(),
        0
    );
}

#[test]
fn stock_brushes_and_pens_have_typed_metadata_and_process_lifetime() {
    let (mut user, mut process) = setup();
    let null = user
        .object_call(&mut process, "GetStockObject", &[5])
        .unwrap();
    assert_ne!(null, 0);
    assert_eq!(
        user.object_call(&mut process, "GetStockObject", &[5])
            .unwrap(),
        null
    );
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[null, 0, 0])
            .unwrap(),
        12
    );
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[null, 12, 0x20200])
            .unwrap(),
        12
    );
    assert_eq!(words::<_, 3>(&process, 0x20200).unwrap(), [1, 0, 0]);
    assert_eq!(
        user.object_call(&mut process, "DeleteObject", &[null])
            .unwrap(),
        1
    );
    assert_eq!(
        user.object_call(&mut process, "DeleteObject", &[null])
            .unwrap(),
        1
    );
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[null, 0, 0])
            .unwrap(),
        12
    );
    let pen = user
        .object_call(&mut process, "GetStockObject", &[8])
        .unwrap();
    assert_ne!(pen, null);
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[pen, 16, 0x20200])
            .unwrap(),
        16
    );
    assert_eq!(words::<_, 4>(&process, 0x20200).unwrap(), [5, 1, 0, 0]);
    let mut other = User::default();
    assert_eq!(
        other
            .object_call(&mut process, "GetObjectA", &[null, 0, 0])
            .unwrap(),
        0
    );
}

#[test]
fn stock_object_errors_and_partial_metadata_writes_are_bounded() {
    let (mut user, mut process) = setup();
    assert_eq!(
        user.object_call(&mut process, "GetStockObject", &[u32::MAX])
            .unwrap(),
        0
    );
    assert_eq!(user.take_error(), Some(87));
    let brush = user
        .object_call(&mut process, "GetStockObject", &[0])
        .unwrap();
    process.write(0x20200, &[0xa5; 16]).unwrap();
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[brush, 4, 0x20200])
            .unwrap(),
        4
    );
    assert_eq!(words::<_, 2>(&process, 0x20200).unwrap(), [0, 0xa5a5a5a5]);
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[brush, 12, 0x20201])
            .unwrap(),
        0
    );
    let dc = user.dc_call("GetDC", &[0], 1).unwrap();
    assert_eq!(
        user.object_call(&mut process, "DeleteObject", &[dc])
            .unwrap(),
        0
    );
    assert_eq!(user.dc_call("GetDeviceCaps", &[dc, 8], 1).unwrap(), 800);
    assert!(user
        .object_call(&mut process, "GetObjectA", &[brush, 12, 0xfffffff0])
        .is_err());
}

#[test]
fn dc_map_modes_preserve_state_on_invalid_unsupported_and_stale_calls() {
    let (mut user, _) = setup();
    let first = user.dc_call("GetDC", &[0], 1).unwrap();
    let second = user.dc_call("GetDC", &[0], 1).unwrap();
    assert_eq!(user.dc_call("GetMapMode", &[first], 1).unwrap(), 1);
    assert_eq!(user.dc_call("SetMapMode", &[first, 1], 1).unwrap(), 1);
    for mode in [0, 9, u32::MAX] {
        assert_eq!(user.dc_call("SetMapMode", &[first, mode], 1).unwrap(), 0);
        assert_eq!(user.take_error(), Some(87));
    }
    for mode in 2..=8 {
        assert_eq!(user.dc_call("SetMapMode", &[first, mode], 1).unwrap(), 0);
        assert_eq!(user.take_error(), Some(50));
    }
    assert_eq!(user.dc_call("GetMapMode", &[first], 1).unwrap(), 1);
    assert_eq!(user.dc_call("GetMapMode", &[second], 1).unwrap(), 1);
    user.dc_call("ReleaseDC", &[0, first], 1).unwrap();
    assert_eq!(user.dc_call("GetMapMode", &[first], 1).unwrap(), 0);
    assert_eq!(user.take_error(), Some(6));
    assert_eq!(user.dc_call("SetMapMode", &[first, 1], 1).unwrap(), 0);
    assert_eq!(user.take_error(), Some(6));
}

#[test]
fn dc_background_and_text_attributes_return_previous_values_and_reset_per_lease() {
    let (mut user, _) = setup();
    let dc = user.dc_call("GetDC", &[0], 1).unwrap();
    assert_eq!(user.dc_call("GetBkMode", &[dc], 1).unwrap(), 2);
    assert_eq!(user.dc_call("SetBkMode", &[dc, 1], 1).unwrap(), 2);
    assert_eq!(user.dc_call("GetBkMode", &[dc], 1).unwrap(), 1);
    assert_eq!(user.dc_call("GetBkColor", &[dc], 1).unwrap(), 0xffffff);
    assert_eq!(user.dc_call("GetTextColor", &[dc], 1).unwrap(), 0);
    assert_eq!(
        user.dc_call("SetBkColor", &[dc, 0x112233], 1).unwrap(),
        0xffffff
    );
    assert_eq!(user.dc_call("GetBkColor", &[dc], 1).unwrap(), 0x112233);
    assert_eq!(
        user.dc_call("SetTextColor", &[dc, 0x01000007], 1).unwrap(),
        0
    );
    assert_eq!(user.dc_call("GetTextColor", &[dc], 1).unwrap(), 0x01000007);
    for mode in [0, 3, u32::MAX] {
        assert_eq!(user.dc_call("SetBkMode", &[dc, mode], 1).unwrap(), 0);
        assert_eq!(user.take_error(), Some(87));
    }
    assert_eq!(user.dc_call("GetBkMode", &[dc], 1).unwrap(), 1);
    user.dc_call("ReleaseDC", &[0, dc], 1).unwrap();
    let fresh = user.dc_call("GetDC", &[0], 1).unwrap();
    assert_eq!(user.dc_call("GetBkMode", &[fresh], 1).unwrap(), 2);
    assert_eq!(user.dc_call("GetBkColor", &[fresh], 1).unwrap(), 0xffffff);
    assert_eq!(user.dc_call("GetTextColor", &[fresh], 1).unwrap(), 0);
    for name in ["GetBkColor", "GetTextColor"] {
        assert_eq!(user.dc_call(name, &[dc], 1).unwrap(), u32::MAX);
        assert_eq!(user.take_error(), Some(6));
    }
    for name in ["SetBkColor", "SetTextColor"] {
        assert_eq!(user.dc_call(name, &[dc, 0], 1).unwrap(), u32::MAX);
        assert_eq!(user.take_error(), Some(6));
    }
    assert_eq!(user.dc_call("GetBkMode", &[dc], 1).unwrap(), 0);
    assert_eq!(user.take_error(), Some(6));
}

#[test]
fn truecolor_palette_realization_is_zero_and_selected_palettes_stay_alive() {
    let (mut user, mut process) = setup();
    let dc = user.dc_call("GetDC", &[0], 1).unwrap();
    let default = user
        .object_call(&mut process, "GetStockObject", &[15])
        .unwrap();
    assert_ne!(default, 0);
    assert_eq!(user.dc_call("RealizePalette", &[dc], 1).unwrap(), 0);
    assert_eq!(
        user.object_call(&mut process, "GetPaletteEntries", &[default, 0, 0, 0])
            .unwrap(),
        20
    );
    assert_eq!(
        user.object_call(&mut process, "GetObjectA", &[default, 0, 0])
            .unwrap(),
        2
    );
    process
        .write(0x20200, &[0, 3, 2, 0, 12, 34, 56, 0, 90, 80, 70, 4])
        .unwrap();
    let palette = user
        .object_call(&mut process, "CreatePalette", &[0x20200])
        .unwrap();
    assert_ne!(palette, 0);
    assert_eq!(
        user.dc_call("SelectPalette", &[dc, palette, 0], 1).unwrap(),
        default
    );
    assert_eq!(user.dc_call("RealizePalette", &[dc], 1).unwrap(), 0);
    assert_eq!(user.dc_call("RealizePalette", &[dc], 1).unwrap(), 0);
    assert_eq!(
        user.object_call(&mut process, "DeleteObject", &[palette])
            .unwrap(),
        0
    );
    process.write(0x20300, &[0xa5; 12]).unwrap();
    assert_eq!(
        user.object_call(&mut process, "GetPaletteEntries", &[palette, 1, 3, 0x20300])
            .unwrap(),
        1
    );
    let mut bytes = [0; 8];
    process.read(0x20300, &mut bytes).unwrap();
    assert_eq!(bytes, [90, 80, 70, 4, 0xa5, 0xa5, 0xa5, 0xa5]);
    assert_eq!(
        user.dc_call("SelectPalette", &[dc, default, 1], 1).unwrap(),
        palette
    );
    assert_eq!(
        user.object_call(&mut process, "DeleteObject", &[palette])
            .unwrap(),
        1
    );
    assert_eq!(
        user.dc_call("SelectPalette", &[dc, palette, 0], 1).unwrap(),
        0
    );
    user.dc_call("ReleaseDC", &[0, dc], 1).unwrap();
    assert_eq!(user.dc_call("RealizePalette", &[dc], 1).unwrap(), u32::MAX);
}

#[test]
fn palette_api_rejects_wrong_types_and_bad_records_without_fake_realization() {
    let (mut user, mut process) = setup();
    let dc = user.dc_call("GetDC", &[0], 1).unwrap();
    let brush = user
        .object_call(&mut process, "GetStockObject", &[5])
        .unwrap();
    assert_eq!(
        user.dc_call("SelectPalette", &[dc, brush, 0], 1).unwrap(),
        0
    );
    assert_eq!(
        user.object_call(&mut process, "GetPaletteEntries", &[brush, 0, 0, 0])
            .unwrap(),
        0
    );
    for bytes in [[0, 0, 2, 0], [0, 3, 0, 0], [0, 3, 1, 1]] {
        process.write(0x20200, &bytes).unwrap();
        assert_eq!(
            user.object_call(&mut process, "CreatePalette", &[0x20200])
                .unwrap(),
            0
        );
    }
    user.display.set_mode((640, 480, 8));
    assert_eq!(user.dc_call("RealizePalette", &[dc], 1).unwrap(), u32::MAX);
    assert_eq!(user.take_error(), Some(50));
}

#[test]
fn translate_message_generates_characters_from_consumed_key_state() {
    let (mut user, mut process) = setup();
    let hwnd = created(&mut user, &mut process);
    user.set_focus(hwnd);
    user.messages.clear();
    // A release can arrive before the guest consumes the preceding keydown.
    user.key_input(0x10, true, false).unwrap();
    user.key_input(0x41, true, false).unwrap();
    user.key_input(0x10, false, false).unwrap();
    for _ in 0..2 {
        assert_eq!(user.peek(&mut process, &[0x20200, 0, 0, 0, 1]).unwrap(), 1);
        user.call(&mut process, "TranslateMessage", &[0x20200])
            .unwrap();
    }
    let message = user.messages.back().unwrap();
    assert_eq!(
        (message.hwnd, message.id, message.wparam),
        (hwnd, 0x102, 65)
    );
    assert_eq!(message.lparam, 1 | (0x1e << 16));
    assert!(!message.input);
    user.messages.clear();
    user.input.keyboard.queued[0x10] = 0;
    user.input.keyboard.queued[0x14] = 1;
    assert_eq!(user.input.keyboard.character(0x41), Some(65));
    user.input.keyboard.queued[0x10] = 0x80;
    assert_eq!(user.input.keyboard.character(0x41), Some(97));
    assert_eq!(user.input.keyboard.character(0xbd), Some(95));
    user.input.keyboard.queued[0x11] = 0x80;
    assert_eq!(user.input.keyboard.character(0x41), Some(1));
    user.input.keyboard.queued[0x11] = 0;
    user.input.keyboard.queued[0x10] = 0;
    user.input.keyboard.queued[0x14] = 0;
    let key = Message::new(hwnd, 0x104, 0x41, 1 | (1 << 29));
    assert_eq!(user.translate_message(key).unwrap(), 1);
    assert_eq!(user.messages.back().unwrap().id, 0x106);
    let length = user.messages.len();
    for id in [0x101, 0x105, 0x200] {
        user.translate_message(Message { id, ..key }).unwrap();
    }
    user.translate_message(Message {
        id: 0x100,
        wparam: 0x25,
        ..key
    })
    .unwrap();
    assert_eq!(user.messages.len(), length);
}
