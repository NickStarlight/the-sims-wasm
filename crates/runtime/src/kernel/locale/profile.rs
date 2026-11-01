//! Fixed English-US defaults for the virtual process, independent of host user overrides.
//! LCTYPE meanings: https://learn.microsoft.com/windows/win32/intl/locale-information-constants
const DAYS: [&str; 7] = [
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
    "Sunday",
];
const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

pub(super) fn value(kind: u32) -> Option<(&'static str, Option<u32>)> {
    let extended = extended_value(kind);
    if let Some(text) = super::data::field(kind) {
        return Some((
            text,
            extended.and_then(|(_, number)| number).map(|_| {
                u32::from_str_radix(text, if matches!(kind, 1 | 9) { 16 } else { 10 }).unwrap()
            }),
        ));
    }
    extended
}

fn extended_value(kind: u32) -> Option<(&'static str, Option<u32>)> {
    let text = match kind {
        1 | 9 => return Some(("0409", Some(0x409))),
        2 => "English (United States)",
        3 => "ENU",
        4 | 0x1001 => "English",
        6 | 8 | 0x1002 => "United States",
        7 => "USA",
        // Numeric identity, measurement, digit and sign-position values.
        5 | 10 | 0x0d | 0x12 | 0x24 | 0x54 | 0x56 | 0x1009 | 0x1010 => return Some(("1", Some(1))),
        11 => return Some(("437", Some(437))),
        0x1004 => return Some(("1252", Some(1252))),
        0x1011 => return Some(("10000", Some(10000))),
        0x11 | 0x19 | 0x1a => return Some(("2", Some(2))),
        0x52 => return Some(("3", Some(3))), // sign immediately before currency symbol
        0x100c => return Some(("6", Some(6))), // Sunday; Monday is index zero
        0x1b | 0x1c | 0x21..=0x23 | 0x25..=0x27 | 0x53 | 0x55 | 0x57 | 0x1005 | 0x100b | 0x100d => {
            return Some(("0", Some(0)))
        }
        0x0c | 0x0f | 0x17 => ",", // list and thousands separators
        0x0e | 0x16 => ".",        // decimal and monetary decimal separators
        0x10 | 0x18 => "3;0",      // repeat groups of three
        0x13 => "0123456789",
        0x14 => "$",
        0x15 => "USD",
        0x1d => "/",
        0x1e => ":",
        0x1f => "M/d/yyyy",
        0x20 => "dddd, MMMM d, yyyy",
        0x1003 => "h:mm:ss tt",
        0x28 => "AM",
        0x29 => "PM",
        0x2a..=0x30 => DAYS[(kind - 0x2a) as usize],
        0x31..=0x37 => &DAYS[(kind - 0x31) as usize][..3],
        0x38..=0x43 => MONTHS[(kind - 0x38) as usize],
        0x44..=0x4f => &MONTHS[(kind - 0x44) as usize][..3],
        0x50 | 0x100e | 0x100f => "", // no positive sign or thirteenth Gregorian month
        0x51 => "-",
        0x59 => "en",
        0x5a => "US",
        0x5c => "en-US",
        0x1006 => "MMMM yyyy",
        _ => return None,
    };
    Some((text, None))
}

#[cfg(test)]
#[path = "../../../tests/unit/kernel/locale/profile.rs"]
mod tests;
