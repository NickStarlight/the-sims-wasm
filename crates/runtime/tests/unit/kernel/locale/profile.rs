use super::*;
#[test]
fn numeric_currency_and_calendar_values_form_one_consistent_profile() {
    for (kind, expected) in [
        (0x0e, "."),
        (0x0f, ","),
        (0x10, "3;0"),
        (0x14, "$"),
        (0x15, "USD"),
        (0x1f, "M/d/yy"),
        (0x1003, "h:mm:ss tt"),
        (0x28, "AM"),
        (0x29, "PM"),
    ] {
        assert_eq!(value(kind).unwrap().0, expected);
    }
    assert_eq!(value(0x19), Some(("2", Some(2))));
    assert_eq!(value(0x1b), Some(("0", Some(0))));
    assert_eq!(value(0x1c), Some(("0", Some(0))));
    assert_eq!(value(0x53), Some(("0", Some(0))));
    assert_eq!(value(0x24), Some(("0", Some(0))));
    for index in 0..7 {
        assert_eq!(
            value(0x31 + index).unwrap().0,
            &value(0x2a + index).unwrap().0[..3]
        );
    }
    for index in 0..12 {
        assert_eq!(
            value(0x44 + index).unwrap().0,
            &value(0x38 + index).unwrap().0[..3]
        );
    }
    assert_eq!(value(0x100e), Some(("", None)));
    assert!(value(0xffff).is_none());
}
