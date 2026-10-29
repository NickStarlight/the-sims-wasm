use super::*;

#[test]
fn zero_filled_entry_stops_at_executable_boundary() {
    let memory = GuestMem::new();
    let error = split_blocks(
        &memory,
        0,
        0x1000,
        Some((0x1000, 0x1200)),
        &[(0x1000, 0x1200)],
        &mut Vec::new(),
    )
    .err()
    .unwrap();
    assert!(
        error.contains("outside mapped executable sections"),
        "{error}"
    );
}

#[test]
fn long_linear_decode_has_an_instruction_budget() {
    let memory = GuestMem::new();
    let error = split_blocks(
        &memory,
        0,
        0x1000,
        None,
        &[(0x1000, 0x100000)],
        &mut Vec::new(),
    )
    .err()
    .unwrap();
    assert!(error.contains("instruction budget"), "{error}");
}

#[test]
fn instruction_cannot_read_across_a_section_gap() {
    let mut memory = GuestMem::new();
    memory.write(0x1000, &[0xb8]); // MOV EAX, imm32 needs five bytes.
    let error = split_blocks(
        &memory,
        0,
        0x1000,
        None,
        &[(0x1000, 0x1001)],
        &mut Vec::new(),
    )
    .err()
    .unwrap();
    assert!(
        error.contains("crosses executable section boundary"),
        "{error}"
    );
}
