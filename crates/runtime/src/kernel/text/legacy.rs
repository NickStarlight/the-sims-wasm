//! Original single-byte comparison/mapping contracts with checked guest reads.
use super::*;
use crate::kernel::{manifest::Call, Kernel};
fn units<M: GuestMemory>(p: &Process<M>, address: u32, count: i32, wide: bool) -> Result<Vec<u16>> {
    let width = if wide { 2 } else { 1 };
    let limit = if count < 0 { 1_048_576 } else { count as u32 };
    if limit > 1_048_576 {
        return Err(Error::InvalidRange);
    }
    let mut units = Vec::new();
    for i in 0..limit {
        let at = address.checked_add(i * width).ok_or(Error::InvalidRange)?;
        let mut b = [0; 2];
        p.read(at, &mut b[..width as usize])?;
        let v = u16::from_le_bytes(b);
        if count < 0 && v == 0 {
            return Ok(units);
        }
        units.push(v);
    }
    if count < 0 {
        return Err(Error::InvalidRange);
    }
    Ok(units)
}
fn fold(v: u16) -> u16 {
    if (65..=90).contains(&v) {
        v + 32
    } else {
        v
    }
}
impl Kernel {
    pub(in crate::kernel) fn legacy_text<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        call: Call,
        a: &[u32],
    ) -> Result<u32> {
        match call {
            Call::LstrlenA => {
                if a[0] == 0 {
                    return Ok(0);
                }
                let mut n = 0;
                while n < 4096 {
                    let mut b = [0];
                    p.read(a[0].checked_add(n).ok_or(Error::InvalidRange)?, &mut b)?;
                    if b[0] == 0 {
                        break;
                    }
                    n += 1;
                }
                Ok(n)
            }
            Call::LstrcpynA => {
                let source = units(p, a[1], -1, false)?;
                let mut bytes: Vec<_> = source
                    .into_iter()
                    .take(a[2].saturating_sub(1) as usize)
                    .map(|v| v as u8)
                    .collect();
                bytes.push(0);
                p.write(a[0], &bytes)?;
                Ok(a[0])
            }
            Call::LCMapStringA => {
                if a[2] == 0 || a[3] == 0 || (a[5] as i32) < 0 {
                    return Ok(self.fail(87));
                }
                let flags = a[1] & !0x40000000;
                let case = flags & 0x300 != 0;
                if !case && (flags & !0xb00 != 0 || flags & 0x800 == 0) {
                    return Ok(self.fail(1004));
                }
                let mut input = units(p, a[2], a[3] as i32, false)?;
                if (a[3] as i32) < 0 {
                    input.push(0);
                }
                if case && a[5] == 0 {
                    return Ok(input.len() as u32);
                }
                if case && (a[5] as usize) < input.len() {
                    return Ok(self.fail(122));
                }
                let bytes: Vec<_> = input
                    .into_iter()
                    .take(a[5] as usize)
                    .map(|v| {
                        if !case {
                            v as u8
                        } else if flags & 0x200 != 0 {
                            if (97..=122).contains(&v) {
                                (v - 32) as u8
                            } else {
                                v as u8
                            }
                        } else {
                            fold(v) as u8
                        }
                    })
                    .collect();
                p.write(a[4], &bytes)?;
                Ok(bytes.len() as u32)
            }
            _ => {
                let (left, llen, right, rlen, flags, wide) = if call == Call::LstrcmpiA {
                    (a[0], -1, a[1], -1, 1, false)
                } else {
                    (
                        a[2],
                        a[3] as i32,
                        a[4],
                        a[5] as i32,
                        a[1],
                        call == Call::CompareStringW,
                    )
                };
                let result: u32 = if left == 0 || right == 0 {
                    0
                } else {
                    let mut left = units(p, left, llen, wide)?;
                    let mut right = units(p, right, rlen, wide)?;
                    if flags & 1 != 0 {
                        left.iter_mut().for_each(|v| *v = fold(*v));
                        right.iter_mut().for_each(|v| *v = fold(*v));
                    }
                    match left.cmp(&right) {
                        std::cmp::Ordering::Less => 1,
                        std::cmp::Ordering::Equal => 2,
                        std::cmp::Ordering::Greater => 3,
                    }
                };
                Ok(if call == Call::LstrcmpiA {
                    result.wrapping_sub(2)
                } else {
                    result
                })
            }
        }
    }
}
