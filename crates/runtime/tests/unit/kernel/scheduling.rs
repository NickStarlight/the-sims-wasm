use super::*;
use crate::SparseMemory;
fn setup() -> (Kernel, Process<SparseMemory>) {
    (
        Kernel::default(),
        Process::new(SparseMemory::default(), 0x10000, 0x1000000).unwrap(),
    )
}
fn call(k: &mut Kernel, p: &mut Process<SparseMemory>, name: &str, a: &[u32]) -> u32 {
    k.call(p, resolve("kernel32", name).unwrap().id, a).unwrap()
}
fn wait(
    k: &mut Kernel,
    p: &mut Process<SparseMemory>,
    name: &str,
    a: &[u32],
    now: f64,
) -> ScheduleOutcome {
    k.schedule_call(p, resolve("kernel32", name).unwrap().id, a, now)
        .unwrap()
}
#[test]
fn signal_consumption_deadlines_and_cancel_are_owned() {
    let (mut k, mut p) = setup();
    assert!(!k.has_ready_waits());
    let e = call(&mut k, &mut p, "CreateEventA", &[0, 0, 0, 0]);
    let ScheduleOutcome::Wait { token, .. } =
        wait(&mut k, &mut p, "WaitForSingleObject", &[e, 50], 10.)
    else {
        panic!()
    };
    assert!(!k.has_ready_waits());
    call(&mut k, &mut p, "SetEvent", &[e]);
    assert!(
        k.has_ready_waits(),
        "signaling completes waits without a clock update"
    );
    assert!(k.has_ready_waits(), "the query does not consume results");
    k.advance_time(100.).unwrap();
    let ready = k.take_ready_waits();
    assert_eq!(ready.len(), 1);
    assert_eq!((ready[0].token, ready[0].value), (token, 0));
    assert!(!k.has_ready_waits());
    assert!(k.take_ready_waits().is_empty());
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[e, 0], 100.),
        ScheduleOutcome::Return { value: 258 }
    );
    let ScheduleOutcome::Wait { token, .. } = wait(&mut k, &mut p, "Sleep", &[10], 100.) else {
        panic!()
    };
    assert!(k.cancel_wait(token));
    k.advance_time(120.).unwrap();
    assert!(k.take_ready_waits().is_empty());
}
#[test]
fn suspended_threads_join_and_close_before_exit_preserve_execution() {
    let (mut k, mut p) = setup();
    let handle = call(
        &mut k,
        &mut p,
        "CreateThread",
        &[0, 4096, 0x401000, 42, 4, 0],
    );
    assert!(k.take_runnable_threads().is_empty());
    assert_eq!(call(&mut k, &mut p, "ResumeThread", &[handle]), 1);
    let descriptors = k.take_runnable_threads();
    assert_eq!(descriptors.len(), 1);
    let d = &descriptors[0];
    let mut bytes = [0; 4];
    p.read(d.teb + 0x24, &mut bytes).unwrap();
    assert_eq!(u32::from_le_bytes(bytes), d.id);
    assert_eq!(d.parameter, 42);
    let ScheduleOutcome::Wait { token, .. } = wait(
        &mut k,
        &mut p,
        "WaitForSingleObject",
        &[handle, u32::MAX],
        0.,
    ) else {
        panic!()
    };
    assert_eq!(call(&mut k, &mut p, "CloseHandle", &[handle]), 1);
    k.finish_thread(d.id, 7).unwrap();
    assert!(k.has_ready_waits(), "thread exit signals joins immediately");
    let ready = k.take_ready_waits();
    assert_eq!((ready[0].token, ready[0].value), (token, 0));
}
#[test]
fn contended_critical_section_transfers_ownership_before_wake() {
    let (mut k, mut p) = setup();
    p.map(0x1000, &[0; 24], true).unwrap();
    call(&mut k, &mut p, "InitializeCriticalSection", &[0x1000]);
    wait(&mut k, &mut p, "EnterCriticalSection", &[0x1000], 0.);
    k.set_thread(2).unwrap();
    let ScheduleOutcome::Wait { token, .. } =
        wait(&mut k, &mut p, "EnterCriticalSection", &[0x1000], 0.)
    else {
        panic!()
    };
    k.set_thread(1).unwrap();
    assert!(!k.has_ready_waits());
    call(&mut k, &mut p, "LeaveCriticalSection", &[0x1000]);
    assert!(
        k.has_ready_waits(),
        "lock transfer does not require advancing time"
    );
    assert_eq!(k.locks[&0x1000].owner, Some(2));
    assert_eq!(k.take_ready_waits()[0].token, token);
}

#[test]
fn recursive_mutexes_transfer_ownership_and_named_handles_share_lifetime() {
    let (mut k, mut p) = setup();
    p.map(0x1000, b"game mutex\0", true).unwrap();
    let mutex = call(&mut k, &mut p, "CreateMutexA", &[0, 1, 0x1000]);
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[mutex, 0], 0.),
        ScheduleOutcome::Return { value: 0 }
    );
    k.set_thread(2).unwrap();
    let other = call(&mut k, &mut p, "CreateMutexA", &[0, 1, 0x1000]);
    assert_ne!(other, mutex);
    assert_eq!(call(&mut k, &mut p, "GetLastError", &[]), 183);
    assert_eq!(call(&mut k, &mut p, "ReleaseMutex", &[other]), 0);
    assert_eq!(call(&mut k, &mut p, "GetLastError", &[]), 288);
    assert_eq!(call(&mut k, &mut p, "SetEvent", &[other]), 0);
    let ScheduleOutcome::Wait { token, .. } =
        wait(&mut k, &mut p, "WaitForSingleObject", &[other, 50], 0.)
    else {
        panic!()
    };
    k.set_thread(1).unwrap();
    assert_eq!(call(&mut k, &mut p, "ReleaseMutex", &[mutex]), 1);
    assert!(
        !k.has_ready_waits(),
        "recursive mutex still belongs to its owner"
    );
    assert!(
        k.take_ready_waits().is_empty(),
        "recursive ownership remains held"
    );
    assert_eq!(call(&mut k, &mut p, "ReleaseMutex", &[mutex]), 1);
    assert!(
        k.has_ready_waits(),
        "final release makes the mutex waiter ready immediately"
    );
    let ready = k.take_ready_waits();
    assert_eq!(
        (ready[0].token, ready[0].thread_id, ready[0].value),
        (token, 2, 0)
    );
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[mutex, 0], 1.),
        ScheduleOutcome::Return { value: 258 }
    );
    assert_eq!(call(&mut k, &mut p, "CloseHandle", &[mutex]), 1);
    k.set_thread(2).unwrap();
    assert_eq!(call(&mut k, &mut p, "ReleaseMutex", &[other]), 1);
    assert_eq!(
        call(&mut k, &mut p, "CreateEventA", &[0, 1, 1, 0x1000]),
        0,
        "event and mutex names share a namespace"
    );
    assert_eq!(call(&mut k, &mut p, "GetLastError", &[]), 6);
    assert_eq!(call(&mut k, &mut p, "CloseHandle", &[other]), 1);
    assert_ne!(
        call(&mut k, &mut p, "CreateEventA", &[0, 1, 1, 0x1000]),
        0,
        "last close releases the name"
    );
}

#[test]
fn ready_query_does_not_advance_deadlines_and_cancel_removes_ready_results() {
    let (mut k, mut p) = setup();
    let ScheduleOutcome::Wait { token, .. } = wait(&mut k, &mut p, "Sleep", &[10], 100.) else {
        panic!()
    };
    assert!(!k.has_ready_waits());
    k.advance_time(109.).unwrap();
    assert!(!k.has_ready_waits());
    k.advance_time(110.).unwrap();
    assert!(k.has_ready_waits());
    assert!(k.cancel_wait(token));
    assert!(!k.has_ready_waits());
    assert!(k.take_ready_waits().is_empty());

    // A zero sleep still follows the wait protocol; reading readiness cannot complete it.
    let ScheduleOutcome::Wait { token, .. } = wait(&mut k, &mut p, "Sleep", &[0], 200.) else {
        panic!()
    };
    assert!(!k.has_ready_waits());
    k.advance_time(200.).unwrap();
    assert!(k.has_ready_waits());
    assert_eq!(k.take_ready_waits()[0].token, token);
    assert!(!k.has_ready_waits());
}

#[test]
fn mutex_wait_all_is_atomic_timeouts_do_not_acquire_and_exit_abandons() {
    let (mut k, mut p) = setup();
    let mutex = call(&mut k, &mut p, "CreateMutexA", &[0, 0, 0]);
    let event = call(&mut k, &mut p, "CreateEventA", &[0, 0, 0, 0]);
    let handles: Vec<_> = [mutex, event]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    p.map(0x1000, &handles, true).unwrap();
    let ScheduleOutcome::Wait { token, .. } = wait(
        &mut k,
        &mut p,
        "WaitForMultipleObjects",
        &[2, 0x1000, 1, 10],
        0.,
    ) else {
        panic!()
    };
    k.set_thread(2).unwrap();
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[mutex, 0], 0.),
        ScheduleOutcome::Return { value: 0 },
        "wait-all must not partially acquire"
    );
    k.advance_time(11.).unwrap();
    let ready = k.take_ready_waits();
    assert_eq!((ready[0].token, ready[0].value), (token, 258));
    call(&mut k, &mut p, "ReleaseMutex", &[mutex]);
    assert!(
        k.take_ready_waits().is_empty(),
        "timed-out wait is no longer pending"
    );
    k.set_thread(1).unwrap();
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[mutex, 0], 11.),
        ScheduleOutcome::Return { value: 0 }
    );
    k.set_thread(3).unwrap();
    let ScheduleOutcome::Wait { token, .. } = wait(
        &mut k,
        &mut p,
        "WaitForSingleObject",
        &[mutex, u32::MAX],
        11.,
    ) else {
        panic!()
    };
    k.finish_thread(1, 0).unwrap();
    let ready = k.take_ready_waits();
    assert_eq!(
        (ready[0].token, ready[0].thread_id, ready[0].value),
        (token, 3, 0x80)
    );
    assert_eq!(call(&mut k, &mut p, "ReleaseMutex", &[mutex]), 1);
    assert_eq!(
        wait(&mut k, &mut p, "WaitForSingleObject", &[mutex, 0], 12.),
        ScheduleOutcome::Return { value: 0 }
    );
}
