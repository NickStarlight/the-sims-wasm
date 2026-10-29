use super::*;

#[test]
fn rgb565_fast_conversion_matches_mask_scaling_exhaustively() {
    let format = Format::new(16).unwrap();
    for value in 0..=u16::MAX as u32 {
        let channel = |mask: u32| {
            (u64::from((value & mask) >> mask.trailing_zeros()) * 255
                / u64::from(mask >> mask.trailing_zeros())) as u8
        };
        assert_eq!(
            RGB565[value as usize],
            [
                channel(format.red),
                channel(format.green),
                channel(format.blue),
                255
            ]
        );
    }
}
