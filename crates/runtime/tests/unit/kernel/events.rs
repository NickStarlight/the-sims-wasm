use super::*;
use crate::SparseMemory;

#[test]
fn event_reset_lifetime_and_immediate_wait_contract() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let call = |kernel: &mut Kernel, process: &mut Process<SparseMemory>, name, args: &[u32]| {
        kernel
            .call(process, resolve("kernel32", name).unwrap().id, args)
            .unwrap()
    };
    let event = call(&mut kernel, &mut process, "CreateEventA", &[0, 0, 0, 0]);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WaitForSingleObject",
            &[event, 0]
        ),
        258
    );
    assert!(kernel
        .call(
            &mut process,
            resolve("kernel32", "WaitForSingleObject").unwrap().id,
            &[event, u32::MAX]
        )
        .is_err());
    assert_eq!(call(&mut kernel, &mut process, "SetEvent", &[event]), 1);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WaitForSingleObject",
            &[event, u32::MAX]
        ),
        0
    );
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WaitForSingleObject",
            &[event, 0]
        ),
        258
    );
    let manual = call(&mut kernel, &mut process, "CreateEventA", &[0, 1, 1, 0]);
    for _ in 0..2 {
        assert_eq!(
            call(
                &mut kernel,
                &mut process,
                "WaitForSingleObject",
                &[manual, 0]
            ),
            0
        );
    }
    assert_eq!(call(&mut kernel, &mut process, "ResetEvent", &[manual]), 1);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WaitForSingleObject",
            &[manual, 0]
        ),
        258
    );
    assert_eq!(call(&mut kernel, &mut process, "CloseHandle", &[event]), 1);
    assert_eq!(call(&mut kernel, &mut process, "CloseHandle", &[event]), 0);
    assert_eq!(kernel.thread().error, 6);
    assert_eq!(
        call(
            &mut kernel,
            &mut process,
            "WaitForSingleObject",
            &[event, 0]
        ),
        u32::MAX
    );
    assert_eq!(
        call(&mut kernel, &mut process, "SetEvent", &[0x40000000]),
        0
    );
    assert_eq!(kernel.thread().error, 6);
}
