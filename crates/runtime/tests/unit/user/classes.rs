use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};

#[test]
fn class_info_preserves_registration_and_owned_strings_after_caller_reuses_buffers() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Gonzo\0").unwrap();
    process.write(0x20200, b"MainMenu\0").unwrap();
    let fields = [
        48, 0x23, 0x401000, 8, 16, 0x400000, 0x123, 0x456, 0x789, 0x20200, 0x20100, 0x987,
    ];
    write_words(&mut process, 0x20000, &fields).unwrap();
    let mut kernel = Kernel::default();
    let register = resolve("user32", "RegisterClassExA").unwrap();
    let atom = kernel.call(&mut process, register.id, &[0x20000]).unwrap();
    assert_ne!(atom, 0);
    process.write(0x20000, &[0xee; 48]).unwrap();
    process.write(0x20100, b"Other\0").unwrap();
    process.write(0x20200, b"Replaced\0").unwrap();
    let query = resolve("user32", "GetClassInfoA").unwrap();
    assert_eq!(
        kernel
            .call(&mut process, query.id, &[0x400000, atom, 0x20300])
            .unwrap(),
        1
    );
    let returned = words::<_, 10>(&process, 0x20300).unwrap();
    assert_eq!(&returned[..8], &fields[1..9]);
    assert_eq!(
        process.read_c_string(returned[8], 256).unwrap(),
        b"MainMenu"
    );
    assert_eq!(process.read_c_string(returned[9], 256).unwrap(), b"Gonzo");
    process.write(0x20400, b"gONZo\0").unwrap();
    let query_ex = resolve("user32", "GetClassInfoExA").unwrap();
    assert_eq!(
        kernel
            .call(&mut process, query_ex.id, &[0x400000, 0x20400, 0x20500])
            .unwrap(),
        1
    );
    let extended = words::<_, 12>(&process, 0x20500).unwrap();
    assert_eq!(extended[0], 48);
    assert_eq!(&extended[1..11], &returned);
    assert_eq!(extended[11], 0x987);
    assert_eq!(
        kernel
            .call(&mut process, query.id, &[0x500000, atom, 0x20300])
            .unwrap(),
        0
    );
    let last_error = resolve("kernel32", "GetLastError").unwrap();
    assert_eq!(kernel.call(&mut process, last_error.id, &[]).unwrap(), 1411);
    assert_eq!(words::<_, 10>(&process, 0x20300).unwrap(), returned);
    assert_eq!(
        kernel
            .call(&mut process, query.id, &[0x400000, 0x20100, 0x20300])
            .unwrap(),
        0
    );
}

#[test]
fn ordinary_registration_preserves_integer_menu_resource_and_zero_small_icon() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x20000, &[0; 4096], true).unwrap();
    process.write(0x20100, b"Window\0").unwrap();
    write_words(
        &mut process,
        0x20000,
        &[0, 0x401000, 0, 4, 0x400000, 22, 33, 44, 101, 0x20100],
    )
    .unwrap();
    let mut user = User::default();
    let atom = user.register(&mut process, 0x20000, false).unwrap();
    assert_eq!(
        user.class_info(&mut process, &[0x400000, atom, 0x20200], true)
            .unwrap(),
        1
    );
    let out = words::<_, 12>(&process, 0x20200).unwrap();
    assert_eq!(out[9], 101);
    assert_eq!(out[11], 0);
    assert!(user
        .class_info(&mut process, &[0x400000, atom, 0x20ff0], false)
        .is_err());
    assert_eq!(user.classes.len(), 1);
}
