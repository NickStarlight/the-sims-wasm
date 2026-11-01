use super::*;
use crate::SparseMemory;

#[test]
fn sizing_terminated_explicit_and_in_place_mapping() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let address = process.heap_alloc(32).unwrap();
    process
        .write(address, &[0xc9, 0, 0x78, 0, 0, 0, 0x41, 0])
        .unwrap();
    let mut kernel = Kernel::default();
    let api = resolve("kernel32", "LCMapStringW").unwrap();
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0, 0x100, address, 1, 0, 0])
            .unwrap(),
        1
    );
    assert_eq!(
        kernel
            .call(
                &mut process,
                api.id,
                &[0, 0x100, address, u32::MAX - 1, 0, 0]
            )
            .unwrap(),
        3
    );
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0, 0x100, address, 4, address, 4])
            .unwrap(),
        4
    );
    let mut result = [0; 8];
    process.read(address, &mut result).unwrap();
    assert_eq!(result, [0xe9, 0, b'x', 0, 0, 0, b'a', 0]);
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0, 0x200, address, 4, address, 3])
            .unwrap(),
        0
    );
    assert_eq!(kernel.thread().error, 122);
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0, 0x300, address, 1, 0, 0])
            .unwrap(),
        0
    );
    assert_eq!(kernel.thread().error, 1004);
    assert!(kernel
        .call(&mut process, api.id, &[0, 0x400, address, 1, 0, 0])
        .is_err());
}

#[test]
fn simple_latin_casing_does_not_expand_or_invent_nls() {
    assert_eq!(case(0xdf, true).unwrap(), 0xdf);
    assert_eq!(case(0xff, true).unwrap(), 0x178);
    assert_eq!(case(0x178, false).unwrap(), 0xff);
    assert_eq!(case(0x20ac, true).unwrap(), 0x20ac);
    assert!(case(0x130, false).is_err());
}
