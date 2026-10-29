use super::*;
use crate::SparseMemory;

fn call(kernel: &mut Kernel, process: &mut Process<SparseMemory>, name: &str, args: &[u32]) -> u32 {
    kernel
        .call(process, resolve("kernel32", name).unwrap().id, args)
        .unwrap()
}
#[test]
fn fixed_allocations_lock_size_reallocate_and_free_without_guest_headers() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let pointer = call(&mut kernel, &mut process, "LocalAlloc", &[0x40, 8]);
    assert_eq!(
        call(&mut kernel, &mut process, "LocalLock", &[pointer]),
        pointer
    );
    assert_eq!(call(&mut kernel, &mut process, "LocalSize", &[pointer]), 8);
    let mut bytes = [1; 8];
    process.read(pointer, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 8]);
    process.write(pointer, b"testdata").unwrap();
    assert_eq!(
        call(&mut kernel, &mut process, "LocalUnlock", &[pointer]),
        0
    );
    assert_eq!(kernel.thread().error, 158);
    assert_eq!(
        call(&mut kernel, &mut process, "LocalReAlloc", &[pointer, 32, 0]),
        0
    );
    process.read(pointer, &mut bytes).unwrap();
    assert_eq!(&bytes, b"testdata");
    let replacement = call(
        &mut kernel,
        &mut process,
        "LocalReAlloc",
        &[pointer, 32, 0x42],
    );
    let mut grown = [1; 32];
    process.read(replacement, &mut grown).unwrap();
    assert_eq!(&grown[..8], b"testdata");
    assert_eq!(&grown[8..], &[0; 24]);
    assert_eq!(
        call(&mut kernel, &mut process, "GlobalFree", &[replacement]),
        0
    );
    assert_eq!(call(&mut kernel, &mut process, "LocalFree", &[0]), 0);
    assert_eq!(
        call(&mut kernel, &mut process, "LocalSize", &[replacement]),
        0
    );
    assert_eq!(kernel.thread().error, 6);
}
#[test]
fn movable_allocations_fail_explicitly_instead_of_returning_fake_handles() {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    for name in ["LocalAlloc", "GlobalAlloc"] {
        let id = resolve("kernel32", name).unwrap().id;
        let error = kernel.call(&mut process, id, &[2, 128]).unwrap_err();
        assert!(error.to_string().contains("fixed allocations only"));
    }
}
