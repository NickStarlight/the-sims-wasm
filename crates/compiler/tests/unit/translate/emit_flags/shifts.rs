//! Execute decoded carry rotations in the browser's Wasm engine (Node/V8).
use crate::translate::test_support::execute_v8;

#[test]
fn carry_rotations_execute_with_x86_results_flags_and_return_stack() {
    execute_v8(
        &[
            ("rcl8", vec![0xd2, 0xd0, 0xc3]),
            ("rcl16", vec![0x66, 0xd3, 0xd0, 0xc3]),
            ("rcl32", vec![0xd3, 0xd0, 0xc3]),
            ("rcr8", vec![0xd2, 0xd8, 0xc3]),
            ("rcr16", vec![0x66, 0xd3, 0xd8, 0xc3]),
            ("rcr32", vec![0xd3, 0xd8, 0xc3]),
        ],
        include_str!("../../../fixtures/carry-rotates.mjs"),
    );
}
