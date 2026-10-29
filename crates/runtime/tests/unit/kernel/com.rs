use super::*;
use crate::SparseMemory;

#[test]
fn apartment_counts_balance_and_changed_mode_does_not_take_a_reference() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let call = |kernel: &mut Kernel, process: &mut Process<SparseMemory>, name, args: &[u32]| {
        kernel
            .call(process, resolve("ole32", name).unwrap().id, args)
            .unwrap()
    };
    assert_eq!(call(&mut kernel, &mut process, "CoInitialize", &[0]), 0);
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 0x0e]),
        1
    );
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 0]),
        0x80010106
    );
    call(&mut kernel, &mut process, "CoUninitialize", &[]);
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 0]),
        0x80010106
    );
    call(&mut kernel, &mut process, "CoUninitialize", &[]);
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 0]),
        0
    );
    kernel.set_thread(2).unwrap();
    assert_eq!(call(&mut kernel, &mut process, "CoInitialize", &[0]), 0);
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitialize", &[1]),
        0x80070057
    );
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 1]),
        0x80070057
    );
    call(&mut kernel, &mut process, "CoUninitialize", &[]);
    call(&mut kernel, &mut process, "CoUninitialize", &[]);
    assert_eq!(
        call(&mut kernel, &mut process, "CoInitializeEx", &[0, 0]),
        0
    );
    let output = process.heap_alloc(4).unwrap();
    process.write(output, &123u32.to_le_bytes()).unwrap();
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "CoCreateInstance",
            &[0, 0, 1, 0, output]
        ),
        0x80040154
    );
    let mut bytes = [1; 4];
    process.read(output, &mut bytes).unwrap();
    assert_eq!(bytes, [0; 4]);
}
