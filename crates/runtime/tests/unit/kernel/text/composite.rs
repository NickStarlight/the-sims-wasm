use super::*;
#[test]
fn composite_policies_and_custom_default() {
    let units: Vec<_> = "e\u{301}x\u{308}\0".encode_utf16().collect();
    assert_eq!(
        encode(&units, 0x220, b'!'),
        (vec![0xe9, b'x', b'!', 0], true)
    );
    assert_eq!(encode(&units, 0x210, b'!'), (vec![0xe9, b'x', 0], false));
    assert_eq!(encode(&units, 0x240, b'!'), (vec![0xe9, b'!', 0], true));
    assert_eq!(encode(&[0xd83d, 0xde00], 0x400, b'?'), (vec![b'?'], true));
    for flags in [0x10, 0x230, 0x280, 0xffff] {
        assert!(!valid_flags(flags));
    }
}
