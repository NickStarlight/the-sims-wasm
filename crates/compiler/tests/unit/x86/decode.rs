use super::*;

fn decode(bytes: &[u8]) -> DResult<Insn> {
    let mut image = GuestMem::new();
    image.write(0, bytes);
    decode_insn(&image, 0x401000, 0x401000)
}

#[test]
fn preserves_narrow_operands_and_immediate_boundaries() {
    let mov = decode(&[0x66, 0xc7, 0x00, 1, 0, 0x90, 0x90]).unwrap();
    assert_eq!(mov.size, 5);
    assert_eq!(mov.ops[1], Operand::Imm(1));
    for opcode in [0xb6, 0xbe] {
        let extend = decode(&[0x66, 0x0f, opcode, 0xc0]).unwrap();
        assert_eq!(extend.ops[0], Operand::Reg { r: 0, size: 2 });
        assert_eq!(extend.ops[1], Operand::Reg { r: 0, size: 1 });
    }
    let add = decode(&[0x83, 0xc0, 0xff]).unwrap();
    assert_eq!(add.ops[1], Operand::Imm(-1));
}

#[test]
fn preserves_prefix_relative_targets_and_condition_codes() {
    let branch = decode(&[0x2e, 0x75, 0x04]).unwrap();
    assert_eq!(branch.size, 3);
    assert_eq!(branch.rel, Some(0x401007));
    assert_eq!(branch.cc, Some(5));
    let set = decode(&[0x0f, 0x97, 0xc4]).unwrap();
    assert_eq!(set.cc, Some(7));
    assert_eq!(set.ops, [Operand::Reg { r: 4, size: 1 }]);
}

#[test]
fn preserves_x87_raw_encoding_and_fs_addresses() {
    let register = decode(&[0xde, 0xf9]).unwrap();
    assert_eq!(register.ops, [Operand::Imm(0xdef9), Operand::St(1)]);
    let memory = decode(&[0x64, 0xdd, 0x44, 0x88, 0x10]).unwrap();
    assert_eq!(
        memory.ops,
        [
            Operand::Imm(0xdd44),
            Operand::Mem(MemOp {
                base: 0,
                index: 1,
                scale: 4,
                disp: 16,
                size: 4,
                fs: true,
            })
        ]
    );
}

#[test]
fn strings_and_xlat_keep_explicit_widths() {
    let string = decode(&[0xf3, 0x66, 0xa5]).unwrap();
    assert_eq!(string.ops, [Operand::Imm(2)]);
    assert_eq!(string.rep, Rep::WhileEqual);
    let byte = decode(&[0x66, 0xa4]).unwrap();
    assert_eq!(byte.ops, [Operand::Imm(1)]);
    assert_eq!(decode(&[0x67, 0xd7]).unwrap().ops[1], Operand::Imm(16));
}

#[test]
fn represents_word_stack_operands_explicitly() {
    assert_eq!(
        decode(&[0x66, 0x50]).unwrap().ops,
        [Operand::Reg { r: 0, size: 2 }]
    );
    assert_eq!(
        decode(&[0x66, 0x58]).unwrap().ops,
        [Operand::Reg { r: 0, size: 2 }]
    );
    assert_eq!(
        decode(&[0x66, 0x68, 0x34, 0x12]).unwrap().ops,
        [Operand::Imm(0x1234), Operand::Imm(2)]
    );
    assert_eq!(
        decode(&[0x66, 0x6a, 0xff]).unwrap().ops,
        [Operand::Imm(-1), Operand::Imm(2)]
    );
    for opcode in [0x9c, 0x9d] {
        assert_eq!(decode(&[0x66, opcode]).unwrap().ops, [Operand::Imm(2)]);
        assert!(decode(&[opcode]).unwrap().ops.is_empty());
    }
}

#[test]
fn rejects_forms_the_emitter_cannot_represent() {
    for bytes in [
        &[0x66, 0xc3][..],
        &[0x67, 0x8b, 0x00],
        &[0x65, 0x8b, 0x00],
        &[0xf0, 0x8b, 0x00],
        &[0xd9, 0x08],
        &[0x0f, 0x28, 0xc0],
        &[0xea, 0, 0, 0, 0, 0, 0],
    ] {
        assert!(decode(bytes).is_err(), "{bytes:x?}");
    }
}
