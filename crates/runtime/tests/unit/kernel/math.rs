use super::mul_div;

#[test]
fn rounds_signed_halves_and_preserves_wide_intermediates() {
    for (a, b, divisor, expected) in [
        (100, 1_000_000, 1_000_000, 100),
        (1, 3, 2, 2),
        (-1, 3, 2, -2),
        (1, 3, -2, -2),
        (-1, 3, -2, 2),
        (i32::MAX, 2, 2, i32::MAX),
        (i32::MIN, 2, 2, i32::MIN),
        (i32::MIN, -1, 1, -1),
        (i32::MAX, 2, 1, -1),
        (1, 1, 0, -1),
    ] {
        assert_eq!(mul_div(a, b, divisor), expected);
    }
}
