//! Signed and unsigned LEB128 encoding for instruction immediates.

pub fn uleb(v: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = v;
    loop {
        let b = (x & 0x7f) as u8;
        x >>= 7;
        if x != 0 {
            out.push(b | 0x80);
        } else {
            out.push(b);
            break;
        }
    }
    out
}

pub fn sleb(v: i32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = v;
    loop {
        let b = (x & 0x7f) as u8;
        x >>= 7;
        let done = (x == 0 && (b & 0x40) == 0) || (x == -1 && (b & 0x40) != 0);
        if done {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out
}

/// Signed LEB128 encoding for 64-bit integer immediates.
pub fn sleb64(v: i64) -> Vec<u8> {
    let mut out = Vec::new();
    let mut x = v;
    loop {
        let b = (x & 0x7f) as u8;
        x >>= 7;
        let done = (x == 0 && (b & 0x40) == 0) || (x == -1 && (b & 0x40) != 0);
        if done {
            out.push(b);
            break;
        }
        out.push(b | 0x80);
    }
    out
}
