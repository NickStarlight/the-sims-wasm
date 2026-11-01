use super::*;
use crate::SparseMemory;

#[test]
fn all_82_locale_callbacks_preserve_original_order_and_reentrant_language_lookup() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let enumeration = resolve("kernel32", "EnumSystemLocalesA").unwrap();
    let info = resolve("kernel32", "GetLocaleInfoA").unwrap();
    let output = process.heap_alloc(64).unwrap();
    let mut outcome = kernel
        .begin_call(&mut process, enumeration.id, &[0x401000, 1])
        .unwrap();
    let mut hash = 0xcbf29ce484222325u64;
    let mut count = 0;
    while let CallOutcome::Callback {
        address,
        args,
        token,
    } = outcome
    {
        assert_eq!(address, 0x401000);
        let identity = process.read_c_string(args[0], 9).unwrap();
        for &byte in identity.iter().chain([0].iter()) {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
        }
        let locale = u32::from_str_radix(std::str::from_utf8(&identity).unwrap(), 16).unwrap();
        kernel
            .call(&mut process, info.id, &[locale, 0x1001, output, 64])
            .unwrap();
        let language = process.read_c_string(output, 64).unwrap();
        assert!(!language.is_empty());
        if locale == 0x401 {
            assert_eq!(language, b"Arabic");
        }
        if locale == 0x407 {
            assert_eq!(language, b"German");
        }
        if locale == 0x1809 {
            assert_eq!(language, b"English");
        }
        kernel
            .call(&mut process, info.id, &[locale, 1, output, 64])
            .unwrap();
        assert_eq!(process.read_c_string(output, 64).unwrap(), identity);
        outcome = kernel.resume_call(&mut process, token, 1).unwrap();
        count += 1;
    }
    assert_eq!(count, 82);
    // Captured from the original data.rs LCID sequence, including each NUL.
    assert_eq!(hash, 0x2177cdd2d5f4ae0d);
    assert_eq!(outcome, CallOutcome::Return { value: 1 });
}

#[test]
fn locale_callback_uses_low_byte_stop_and_keeps_nested_tokens_independent() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let id = resolve("kernel32", "EnumSystemLocalesA").unwrap().id;
    let CallOutcome::Callback { token, .. } =
        kernel.begin_call(&mut process, id, &[0x401000, 1]).unwrap()
    else {
        panic!()
    };
    let CallOutcome::Callback { token: nested, .. } =
        kernel.begin_call(&mut process, id, &[0x402000, 1]).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        kernel.resume_call(&mut process, nested, 0x100).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    let CallOutcome::Callback { token: next, .. } =
        kernel.resume_call(&mut process, token, 0x101).unwrap()
    else {
        panic!()
    };
    assert_eq!(
        kernel.resume_call(&mut process, next, 0).unwrap(),
        CallOutcome::Return { value: 0 }
    );
    assert!(kernel.resume_call(&mut process, nested, 1).is_err());
}

#[test]
fn legacy_locale_fields_keep_original_values_and_lcid_language_rules() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let mut kernel = Kernel::default();
    let id = resolve("kernel32", "GetLocaleInfoA").unwrap().id;
    let out = process.heap_alloc(128).unwrap();
    for kind in 0..=0x1011 {
        if let Some(expected) = super::super::data::field(kind) {
            kernel.call(&mut process, id, &[0, kind, out, 128]).unwrap();
            assert_eq!(
                process.read_c_string(out, 128).unwrap(),
                expected.as_bytes(),
                "kind {kind:#x}"
            );
        }
    }
    for (kind, expected) in [(2, "French"), (4, "French"), (1, "0000080c"), (9, "000c")] {
        kernel
            .call(&mut process, id, &[0x80c, kind, out, 128])
            .unwrap();
        assert_eq!(
            process.read_c_string(out, 128).unwrap(),
            expected.as_bytes()
        );
    }
    assert_eq!(
        kernel
            .call(&mut process, id, &[0, 0xffff, out, 128])
            .unwrap(),
        0
    );
    assert_eq!(kernel.thread().error, 87);
    assert_eq!(super::super::profile::value(0x0c).unwrap().0, ";");
    assert_eq!(super::super::profile::value(0x1f).unwrap().0, "M/d/yy");
}
