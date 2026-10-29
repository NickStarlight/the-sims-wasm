use super::*;
use crate::SparseMemory;

#[test]
fn guest_varargs_cdecl_width_precision_and_unicode() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    process.map(0x1000, &[0; 4096], true).unwrap();
    process
        .write(0x1100, b"%s %04d %#x %.3s %S %% %hd\0")
        .unwrap();
    process.write(0x1200, b"hello\0").unwrap();
    process.write(0x1300, &[0xe9, 0, 0, 0]).unwrap();
    let values: [u32; 6] = [0x1200, (-12i32) as u32, 42, 0x1200, 0x1300, 0xffff];
    let bytes: Vec<_> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
    process.write(0x1400, &bytes).unwrap();
    let api = resolve("user32", "wsprintfA").unwrap();
    assert!(api.variadic);
    assert_eq!((api.arity, api.stack_args), (2, 0));
    let expected = b"hello -012 0x2a hel \xe9 % -1\0";
    let mut kernel = Kernel::default();
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0x1500, 0x1100, 0x1400])
            .unwrap(),
        expected.len() as u32 - 1
    );
    let mut actual = vec![0; expected.len()];
    process.read(0x1500, &mut actual).unwrap();
    assert_eq!(actual, expected);
    assert!(kernel
        .call(&mut process, api.id, &[0x1500, 0x1100])
        .is_err());
}

#[test]
fn output_is_bounded_and_optional_arguments_are_checked() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    process.map(0x1000, &[0; 4096], true).unwrap();
    process.write(0x1100, b"%99999999u\0").unwrap();
    process.write(0x1200, &7u32.to_le_bytes()).unwrap();
    process.write(0x1800, &[0xaa; 1025]).unwrap();
    let api = resolve("user32", "wsprintfA").unwrap();
    let mut kernel = Kernel::default();
    assert_eq!(
        kernel
            .call(&mut process, api.id, &[0x1800, 0x1100, 0x1200])
            .unwrap(),
        1023
    );
    let mut end = [0; 2];
    process.read(0x1bff, &mut end).unwrap();
    assert_eq!(end, [0, 0xaa]);
    assert_eq!(kernel.thread().error, 122);
    assert!(kernel
        .call(&mut process, api.id, &[0x1800, 0x1100, 0xffff0000])
        .is_err());
}
