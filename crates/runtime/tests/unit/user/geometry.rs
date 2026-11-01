use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};

#[test]
fn popup_rect_is_unchanged_but_decorations_and_menus_fail_explicitly() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    let rect = [(-40i32) as u32, (-20i32) as u32, 800, 600];
    write_words(&mut process, 0x20000, &rect).unwrap();
    let mut kernel = Kernel::default();
    let adjust = resolve("user32", "AdjustWindowRect").unwrap();
    assert_eq!(
        kernel
            .call(&mut process, adjust.id, &[0x20000, 0x90000000, 0])
            .unwrap(),
        1
    );
    assert_eq!(words::<_, 4>(&process, 0x20000).unwrap(), rect);
    for (style, menu) in [
        (0x90c00000, 0),
        (0x90040000, 0),
        (0x90000000, 1),
        (0x90200000, 0),
    ] {
        assert_eq!(
            kernel
                .call(&mut process, adjust.id, &[0x20000, style, menu])
                .unwrap(),
            0
        );
        let error = resolve("kernel32", "GetLastError").unwrap();
        assert_eq!(kernel.call(&mut process, error.id, &[]).unwrap(), 120);
        assert_eq!(words::<_, 4>(&process, 0x20000).unwrap(), rect);
    }
    let extended = resolve("user32", "AdjustWindowRectEx").unwrap();
    assert_eq!(
        kernel
            .call(&mut process, extended.id, &[0x20000, 0x90000000, 0, 8])
            .unwrap(),
        1
    );
    for ex_style in [1, 0x100, 0x200, 0x20000] {
        assert_eq!(
            kernel
                .call(
                    &mut process,
                    extended.id,
                    &[0x20000, 0x90000000, 0, ex_style]
                )
                .unwrap(),
            0
        );
    }
}
#[test]
fn rect_requires_complete_readable_and_writable_guest_memory() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 16], false).unwrap();
    let mut user = User::default();
    assert!(matches!(
        user.adjust_rect(&mut process, &[0x20000, 0x90000000, 0], false),
        Err(Error::Protected)
    ));
    assert!(user
        .adjust_rect(&mut process, &[0x20008, 0x90000000, 0], false)
        .is_err());
    assert!(user
        .adjust_rect(&mut process, &[u32::MAX - 7, 0x90000000, 0], false)
        .is_err());
}
