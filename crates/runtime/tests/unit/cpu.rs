use super::*;
use crate::SparseMemory;

const TEB: u32 = 0x2000;

fn process(writable: bool) -> Process<SparseMemory> {
    let mut process = Process::new(SparseMemory::default(), 0x10000, 0x100000).unwrap();
    process.map(TEB, &[0; 0x1000], writable).unwrap();
    process
}

fn registers(top: i32) -> [i32; 16] {
    let mut registers = std::array::from_fn(|index| 0x12340000 + index as i32);
    registers[FP_TOP] = top;
    registers[TEB_BASE] = TEB as i32;
    registers
}

fn st(process: &Process<SparseMemory>, registers: &[i32; 16], offset: usize) -> f64 {
    let address = TEB + FPU_OFFSET + 8 * (((registers[FP_TOP] as usize + offset) & 7) as u32);
    let mut bytes = [0; 8];
    process.read(address, &mut bytes).unwrap();
    f64::from_le_bytes(bytes)
}

fn set_st(process: &mut Process<SparseMemory>, top: i32, offset: usize, value: f64) {
    let address = TEB + FPU_OFFSET + 8 * (((top as usize + offset) & 7) as u32);
    process.write(address, &value.to_le_bytes()).unwrap();
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= expected.abs().max(1e-30) * 1e-14,
        "actual {actual}, expected {expected}"
    );
}

#[test]
fn binary_x87_helpers_pop_in_operand_order_and_wrap_top() {
    for (id, x, y, expected) in [
        (1, -1.0, 1.0, 3.0 * std::f64::consts::FRAC_PI_4),
        (3, 8.0, 2.0, 6.0),
        (5, 1e-20, 2.0, 2e-20 * std::f64::consts::LOG2_E),
    ] {
        let mut process = process(true);
        let before = registers(7);
        set_st(&mut process, 7, 0, x);
        set_st(&mut process, 7, 1, y);
        let after = process.cpu_helper(id, before, 0.0).unwrap();
        assert_eq!(after[FP_TOP], 0);
        close(st(&process, &after, 0), expected);
        let mut expected_registers = before;
        expected_registers[FP_TOP] = 0;
        assert_eq!(
            after, expected_registers,
            "helper must preserve flags/status/ESP/continuation"
        );
    }
}

#[test]
fn unary_and_pushing_x87_helpers_preserve_status_and_small_values() {
    for (id, x, result) in [
        (4, 1e-20, 1e-20 * std::f64::consts::LN_2),
        (6, std::f64::consts::FRAC_PI_6, 0.5),
        (7, std::f64::consts::FRAC_PI_3, 0.5),
    ] {
        let mut process = process(true);
        let before = registers(3);
        set_st(&mut process, 3, 0, x);
        let after = process.cpu_helper(id, before, 0.0).unwrap();
        assert_eq!(after, before);
        close(st(&process, &after, 0), result);
    }
    for id in [2, 8] {
        let mut process = process(true);
        let before = registers(0);
        let x = std::f64::consts::FRAC_PI_4;
        set_st(&mut process, 0, 0, x);
        let after = process.cpu_helper(id, before, 0.0).unwrap();
        assert_eq!(after[FP_TOP], 7);
        close(
            st(&process, &after, 0),
            if id == 2 {
                1.0
            } else {
                std::f64::consts::FRAC_1_SQRT_2
            },
        );
        close(
            st(&process, &after, 1),
            if id == 2 {
                1.0
            } else {
                std::f64::consts::FRAC_1_SQRT_2
            },
        );
        assert_eq!(after[8], before[8]);
        assert_eq!(after[10], before[10]);
        assert_eq!(after[4], before[4]);
    }
}

#[test]
fn cpuid_advertises_only_lowered_instruction_families() {
    let mut process = process(false);
    let mut input = registers(0);
    input[EAX] = 0;
    input[TEB_BASE] = -1; // CPUID must not access the x87 register bank.
    let vendor = process.cpu_helper(10, input, 0.0).unwrap();
    let name: Vec<u8> = [EBX, EDX, ECX]
        .into_iter()
        .flat_map(|r| vendor[r].to_le_bytes())
        .collect();
    assert_eq!(name, b"GenuineIntel");
    assert_eq!(vendor[EAX], 1);
    input[EAX] = 1;
    let features = process.cpu_helper(10, input, 0.0).unwrap();
    assert_eq!(features[EDX], 0x11);
    assert_eq!(
        features[EDX] & ((1 << 8) | (1 << 15) | (1 << 23) | (1 << 25) | (1 << 26)),
        0
    );
    assert_eq!(features[ECX], 0);
    assert_eq!(&features[4..], &input[4..]);
    input[EAX] = i32::MIN;
    let unsupported = process.cpu_helper(10, input, 0.0).unwrap();
    assert_eq!(&unsupported[..4], &[0; 4]);
    assert_eq!(&unsupported[4..], &input[4..]);
}

#[test]
fn timestamp_splits_synthetic_cycles_without_popping_guest_stack() {
    let mut process = process(false);
    let before = registers(0);
    let after = process.cpu_helper(9, before, 2000.25).unwrap();
    assert_eq!(
        (u64::from(after[EDX] as u32) << 32) | u64::from(after[EAX] as u32),
        6_000_750_000
    );
    let mut expected = before;
    expected[EAX] = 6_000_750_000u64 as i32;
    expected[EDX] = 1;
    assert_eq!(after, expected);
    for clock in [-1.0, f64::NAN, f64::INFINITY, f64::MAX] {
        assert_eq!(
            process.cpu_helper(9, before, clock),
            Err(Error::InvalidClock)
        );
    }
}

#[test]
fn helper_errors_do_not_bypass_memory_protection() {
    let mut readonly = process(false);
    let before = registers(0);
    assert_eq!(readonly.cpu_helper(6, before, 0.0), Err(Error::Protected));
    close(st(&readonly, &before, 0), 0.0);
    let mut invalid = before;
    invalid[TEB_BASE] = -1;
    assert_eq!(
        readonly.cpu_helper(1, invalid, 0.0),
        Err(Error::InvalidRange)
    );
    invalid[TEB_BASE] = 0x4000;
    assert_eq!(readonly.cpu_helper(1, invalid, 0.0), Err(Error::Unmapped));
    assert_eq!(
        readonly.cpu_helper(11, before, 0.0),
        Err(Error::UnknownCpuHelper(11))
    );
}
