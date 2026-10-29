use super::*;
use crate::SparseMemory;
fn fixture() -> (Kernel, Process<SparseMemory>, [u32; 16]) {
    let mut p = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    p.map(0x1000, &vec![0; 0x5000], true).unwrap();
    put(&mut p, 0x1000, 0x5800).unwrap();
    put(&mut p, 0x1004, 0x6000).unwrap();
    put(&mut p, 0x1008, 0x2000).unwrap();
    put(&mut p, 0x5800, END).unwrap();
    put(&mut p, 0x5804, 0x401000).unwrap();
    put(&mut p, 0x5000, 0x402000).unwrap();
    let mut r = [0; 16];
    r[ESP] = 0x5000;
    r[TEB] = 0x1000;
    r[0] = 7;
    (Kernel::default(), p, r)
}
#[test]
fn guest_handler_continues_with_context_registers_and_exact_stdcall_return() {
    let (mut k, mut p, r) = fixture();
    let api = resolve("kernel32", "RaiseException").unwrap();
    let action = k
        .begin_exception(&mut p, api.id, &[0x1234, 0, 0, 0], &r)
        .unwrap();
    let ExceptionAction::Invoke {
        token,
        address,
        mut registers,
    } = action
    else {
        panic!()
    };
    assert_eq!(address, 0x401000);
    let sp = registers[ESP];
    let record = word(&p, sp + 4).unwrap();
    let context = word(&p, sp + 12).unwrap();
    assert_eq!(word(&p, record).unwrap(), 0x1234);
    assert_eq!(word(&p, sp + 8).unwrap(), 0x5800);
    put(&mut p, context + 0xb0, 99).unwrap();
    registers[ACTUAL_RETURN] = 0;
    registers[ESP] += 4;
    registers[0] = 0;
    let ExceptionAction::Complete { registers } =
        k.resume_exception(&mut p, token, &registers).unwrap()
    else {
        panic!()
    };
    assert_eq!(registers[ESP], 0x5014);
    assert_eq!(registers[ACTUAL_RETURN], 0x402000);
    assert_eq!(registers[0], 99);
    assert!(k.resume_exception(&mut p, token, &registers).is_err());
}
#[test]
fn unwind_unlinks_processed_frames_and_keeps_the_target_frame() {
    let (mut k, mut p, r) = fixture();
    put(&mut p, 0x5800, 0x5900).unwrap();
    put(&mut p, 0x5900, END).unwrap();
    let api = resolve("kernel32", "RtlUnwind").unwrap();
    let ExceptionAction::Invoke {
        token,
        mut registers,
        ..
    } = k
        .begin_exception(&mut p, api.id, &[0x5900, 0, 0, 42], &r)
        .unwrap()
    else {
        panic!()
    };
    let record = word(&p, registers[ESP] + 4).unwrap();
    assert_eq!(word(&p, record + 4).unwrap(), 2);
    registers[ACTUAL_RETURN] = 0;
    registers[ESP] += 4;
    registers[0] = 1;
    let ExceptionAction::Complete { registers } =
        k.resume_exception(&mut p, token, &registers).unwrap()
    else {
        panic!()
    };
    assert_eq!(word(&p, 0x1000).unwrap(), 0x5900);
    assert_eq!(registers[0], 42);
    assert_eq!(registers[ESP], 0x5014);
}
#[test]
fn native_catch_transfers_preserve_cpu_state_and_tokens_are_thread_owned() {
    let (mut k, mut p, r) = fixture();
    let api = resolve("kernel32", "RaiseException").unwrap();
    let ExceptionAction::Invoke {
        token,
        mut registers,
        ..
    } = k
        .begin_exception(&mut p, api.id, &[0xe06d7363, 1, 0, 0], &r)
        .unwrap()
    else {
        panic!()
    };
    k.set_thread(2).unwrap();
    assert!(k.resume_exception(&mut p, token, &registers).is_err());
    assert!(!k.cancel_exception(token));
    k.set_thread(1).unwrap();
    registers[ACTUAL_RETURN] = 0x404000;
    registers[ESP] = 0x5900;
    registers[0] = 123;
    let ExceptionAction::Complete { registers: actual } =
        k.resume_exception(&mut p, token, &registers).unwrap()
    else {
        panic!()
    };
    assert_eq!(actual, registers);
}
