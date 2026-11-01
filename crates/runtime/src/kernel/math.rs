//! Integer arithmetic exposed by the Windows kernel API.

pub(super) fn mul_div(number: i32, numerator: i32, denominator: i32) -> i32 {
    if denominator == 0 {
        return -1;
    }
    let product = i64::from(number) * i64::from(numerator);
    let divisor = i64::from(denominator);
    let magnitude = (product.abs() + divisor.abs() / 2) / divisor.abs();
    let result = if (product < 0) != (divisor < 0) {
        -magnitude
    } else {
        magnitude
    };
    i32::try_from(result).unwrap_or(-1)
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/math.rs"]
mod tests;
