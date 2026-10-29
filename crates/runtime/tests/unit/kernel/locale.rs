use super::*;
#[test]
fn ascii_classes_cover_the_crt_contract() {
    assert_eq!(classify(b'A'.into()).unwrap() & 0x1ff, UPPER | ALPHA | HEX);
    assert_eq!(classify(b'z'.into()).unwrap() & 0x1ff, LOWER | ALPHA);
    assert_eq!(classify(b'9'.into()).unwrap() & 0x1ff, DIGIT | HEX);
    assert_eq!(classify(b' '.into()).unwrap() & 0x1ff, SPACE | BLANK);
    assert_eq!(
        classify(b'\t'.into()).unwrap() & 0x1ff,
        SPACE | BLANK | CONTROL
    );
    assert_eq!(classify(b'\n'.into()).unwrap() & 0x1ff, SPACE | CONTROL);
    assert_eq!(classify(b'_'.into()).unwrap() & 0x1ff, PUNCT);
    assert_eq!(classify(0).unwrap() & 0x1ff, CONTROL);
    assert_eq!(classify(0xe9).unwrap() & (ALPHA | LOWER), ALPHA | LOWER);
    assert!(classify(0xd800).is_err());
}
