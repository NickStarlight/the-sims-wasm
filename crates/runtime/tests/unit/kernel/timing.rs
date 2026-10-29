use super::*;
use crate::SparseMemory;

#[test]
fn utc_timezone_fills_the_entire_structure_without_touching_neighbors() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let address = process.heap_alloc(174).unwrap();
    process.write(address, &[0xaa; 174]).unwrap();
    let mut kernel = Kernel::default();
    let api = resolve("kernel32", "GetTimeZoneInformation").unwrap();
    assert!(!api.clock);
    assert_eq!(
        kernel.call(&mut process, api.id, &[address + 1]).unwrap(),
        0
    );
    let mut bytes = [0; 174];
    process.read(address, &mut bytes).unwrap();
    assert_eq!(bytes[0], 0xaa);
    assert_eq!(bytes[173], 0xaa);
    for offset in [5, 89] {
        assert_eq!(&bytes[offset..offset + 6], &[b'U', 0, b'T', 0, b'C', 0]);
        bytes[offset..offset + 6].fill(0);
    }
    assert!(bytes[1..173].iter().all(|byte| *byte == 0));
    assert_eq!(kernel.call(&mut process, api.id, &[0]).unwrap(), u32::MAX);
    assert_eq!(kernel.thread().error, 87);
}

#[test]
fn counter_and_tick_apis_share_real_samples_and_tick_wrap() {
    let mut process = Process::new(SparseMemory::default(), 0x1000, 0x100000).unwrap();
    let address = process.heap_alloc(8).unwrap();
    let mut kernel = Kernel::default();
    let frequency = resolve("kernel32", "QueryPerformanceFrequency").unwrap();
    assert!(!frequency.clock);
    assert_eq!(
        kernel.call(&mut process, frequency.id, &[address]).unwrap(),
        1
    );
    let mut bytes = [0; 8];
    process.read(address, &mut bytes).unwrap();
    assert_eq!(u64::from_le_bytes(bytes), FREQUENCY);
    let counter = resolve("kernel32", "QueryPerformanceCounter").unwrap();
    assert!(counter.clock);
    assert!(kernel.call(&mut process, counter.id, &[address]).is_err());
    kernel.set_monotonic_time(1234.567).unwrap();
    kernel.call(&mut process, counter.id, &[address]).unwrap();
    process.read(address, &mut bytes).unwrap();
    assert_eq!(u64::from_le_bytes(bytes), 1_234_567);
    assert!(kernel.call(&mut process, counter.id, &[address]).is_err());
    for (dll, name) in [("kernel32", "GetTickCount"), ("winmm", "timeGetTime")] {
        let api = resolve(dll, name).unwrap();
        assert!(api.clock);
        kernel.set_monotonic_time(4_294_967_301.75).unwrap();
        assert_eq!(kernel.call(&mut process, api.id, &[]).unwrap(), 5);
    }
    for invalid in [1.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(kernel.set_monotonic_time(invalid).is_err());
    }
}
