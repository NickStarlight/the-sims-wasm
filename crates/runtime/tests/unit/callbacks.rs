use super::*;
use crate::{
    kernel::{resolve, Kernel},
    SparseMemory,
};

fn process() -> Process<SparseMemory> {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(0x2000, &[0; 4096], true).unwrap();
    process
}
fn pointers(process: &mut Process<SparseMemory>, address: u32, values: &[u32]) {
    let bytes: Vec<_> = values
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect();
    process.write(address, &bytes).unwrap();
}
fn callback(outcome: CallOutcome, expected: u32) -> u32 {
    let CallOutcome::Callback {
        address,
        args,
        token,
    } = outcome
    else {
        panic!("expected callback");
    };
    assert_eq!(address, expected);
    assert!(args.is_empty());
    token
}

#[test]
fn initializers_skip_nulls_and_nested_calls_have_independent_tokens() {
    let mut process = process();
    pointers(&mut process, 0x2000, &[0, 0x401000, 0, 0x402000]);
    pointers(&mut process, 0x2100, &[0x403000]);
    let mut calls = Callbacks::default();
    let outer = callback(
        calls.initializers(&process, 0x2000, 0x2010).unwrap(),
        0x401000,
    );
    let inner = callback(
        calls.initializers(&process, 0x2100, 0x2104).unwrap(),
        0x403000,
    );
    assert_ne!(outer, inner);
    assert_eq!(
        calls.resume(&process, inner, u32::MAX).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    let next = callback(calls.resume(&process, outer, 123).unwrap(), 0x402000);
    assert_ne!(outer, next);
    assert!(calls.resume(&process, outer, 0).is_err());
    assert_eq!(
        calls.resume(&process, next, 456).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    assert!(calls.pending.is_empty());
}

#[test]
fn resume_reads_updated_pointers_and_read_errors_consume_only_their_token() {
    let mut process = process();
    pointers(&mut process, 0x2000, &[0x401000, 0x402000]);
    pointers(&mut process, 0x2ffc, &[0x403000]);
    let mut calls = Callbacks::default();
    let surviving = callback(
        calls.initializers(&process, 0x2000, 0x2008).unwrap(),
        0x401000,
    );
    let invalid = callback(
        calls.initializers(&process, 0x2ffc, 0x3004).unwrap(),
        0x403000,
    );
    assert_eq!(calls.resume(&process, invalid, 0), Err(Error::Unmapped));
    assert!(!calls.cancel(invalid));
    assert!(calls.resume(&process, invalid, 0).is_err());
    pointers(&mut process, 0x2004, &[0x404000]);
    let next = callback(calls.resume(&process, surviving, 0).unwrap(), 0x404000);
    assert!(calls.cancel(next));
    assert!(!calls.cancel(next));
    assert!(calls.pending.is_empty());
}

#[test]
fn invalid_ranges_and_cancelled_tokens_leave_no_pending_state() {
    let mut process = process();
    let mut calls = Callbacks::default();
    assert_eq!(
        calls.initializers(&process, 0, 0).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    for (start, end) in [(8, 4), (0x2000, 0x2003), (0, MAX_INITIALIZERS * 4 + 4)] {
        assert_eq!(
            calls.initializers(&process, start, end),
            Err(Error::InvalidRange)
        );
    }
    assert_eq!(
        calls.initializers(&process, 0x3000, 0x3004),
        Err(Error::Unmapped)
    );
    assert!(calls.resume(&process, 0, 0).is_err());
    pointers(&mut process, 0x2000, &[0x401000]);
    let token = callback(
        calls.initializers(&process, 0x2000, 0x2004).unwrap(),
        0x401000,
    );
    assert!(calls.cancel(token));
    assert!(calls.resume(&process, token, 0).is_err());
    calls.next_token = u32::MAX;
    assert!(calls.initializers(&process, 0x2000, 0x2004).is_err());
    assert!(calls.pending.is_empty());
}

#[test]
fn kernel_separates_fast_calls_from_serialized_callback_outcomes() {
    let mut kernel = Kernel::default();
    let mut process = process();
    pointers(&mut process, 0x2000, &[0x401000]);
    let init = resolve("msvcrt.dll", "_initterm").unwrap();
    assert!(init.callback);
    assert!(kernel
        .call(&mut process, init.id, &[0x2000, 0x2004])
        .is_err());
    assert!(kernel.begin_call(&mut process, init.id, &[0x2000]).is_err());
    let outcome = kernel
        .begin_call(&mut process, init.id, &[0x2000, 0x2004])
        .unwrap();
    let json = serde_json::to_value(&outcome).unwrap();
    assert_eq!(json["kind"], "callback");
    assert_eq!(json["address"], 0x401000);
    assert_eq!(json["args"], serde_json::json!([]));
    let token = callback(outcome, 0x401000);
    assert_eq!(
        kernel.resume_call(&mut process, token, 99).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    assert!(!kernel.cancel_call(token));
    let ordinary = resolve("kernel32.dll", "GetCurrentProcessId").unwrap();
    assert!(!ordinary.callback);
    let outcome = kernel.begin_call(&mut process, ordinary.id, &[]).unwrap();
    assert_eq!(
        serde_json::to_value(outcome).unwrap(),
        serde_json::json!({"kind":"return","value":1})
    );
}

#[test]
fn pending_limit_is_bounded_and_cancellation_reclaims_capacity() {
    let mut process = process();
    pointers(&mut process, 0x2000, &[0x401000]);
    let mut calls = Callbacks::default();
    let mut tokens = Vec::new();
    for _ in 0..MAX_PENDING_CALLS {
        tokens.push(callback(
            calls.initializers(&process, 0x2000, 0x2004).unwrap(),
            0x401000,
        ));
    }
    assert!(calls.initializers(&process, 0x2000, 0x2004).is_err());
    assert_eq!(calls.pending.len(), MAX_PENDING_CALLS);
    for token in tokens {
        assert!(calls.cancel(token));
    }
    let token = callback(
        calls.initializers(&process, 0x2000, 0x2004).unwrap(),
        0x401000,
    );
    assert_eq!(
        calls.resume(&process, token, 0).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    assert!(calls.pending.is_empty());
}

#[test]
fn enumeration_sequences_stop_on_zero_and_keep_nested_tokens_independent() {
    let process = process();
    let mut calls = Callbacks::default();
    let outer = calls
        .sequence(vec![(0x401000, vec![1]), (0x401000, vec![2])], true, 7)
        .unwrap();
    let CallOutcome::Callback { token, args, .. } = outer else {
        panic!()
    };
    assert_eq!(args, [1]);
    let inner = calls.sequence(vec![(0x402000, vec![])], false, 9).unwrap();
    let inner = callback(inner, 0x402000);
    assert_eq!(
        calls.resume(&process, token, 0).unwrap(),
        CallOutcome::Return { value: 7 }
    );
    assert_eq!(
        calls.resume(&process, inner, 0).unwrap(),
        CallOutcome::Return { value: 9 }
    );
    assert!(calls.pending.is_empty());
    assert!(calls.sequence(vec![(0, vec![])], true, 0).is_err());
}
