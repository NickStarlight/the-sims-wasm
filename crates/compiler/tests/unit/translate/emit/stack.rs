use crate::translate::test_support::execute_v8;

#[test]
fn word_stack_operations_preserve_width_addresses_and_flags() {
    execute_v8(
        &[
            ("movsx_ax_cl", vec![0x66, 0x0f, 0xbe, 0xc1]),
            ("movzx_ax_cl", vec![0x66, 0x0f, 0xb6, 0xc1]),
            ("push_ax", vec![0x66, 0x50]),
            ("pop_ax", vec![0x66, 0x58]),
            ("push_sp", vec![0x66, 0x54]),
            ("pop_sp", vec![0x66, 0x5c]),
            ("push_esp", vec![0x54]),
            ("pop_esp", vec![0x5c]),
            ("push_mem16", vec![0x66, 0xff, 0x34, 0x24]),
            ("pop_mem16", vec![0x66, 0x8f, 0x04, 0x24]),
            ("push_mem32", vec![0xff, 0x34, 0x24]),
            ("pop_mem32", vec![0x8f, 0x04, 0x24]),
            ("push_imm8_16", vec![0x66, 0x6a, 0xff]),
            ("push_imm16", vec![0x66, 0x68, 0x34, 0x12]),
            ("push_flags16", vec![0x66, 0x9c]),
            ("pop_flags16", vec![0x66, 0x9d]),
            ("push_flags32", vec![0x9c]),
            ("pop_flags32", vec![0x9d]),
            (
                "cpu_flags_roundtrip",
                vec![0x66, 0x9c, 0x66, 0x58, 0x66, 0x50, 0x66, 0x9d, 0xc3],
            ),
        ],
        include_str!("../../../fixtures/word-stack.mjs"),
    );
}
