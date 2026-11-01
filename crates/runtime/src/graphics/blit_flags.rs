//! Validation order ported from runtime-rs/graphics/ddraw/blit.rs.
use super::*;
pub(super) fn validate<M: GuestMemory>(
    p: &mut Process<M>,
    surface: &Surface,
    source: Option<&Surface>,
    flags: u32,
    fx: u32,
) -> Result<std::result::Result<u32, u32>> {
    let mut flags = flags & !(0x1ff | 0x80000 | 0xf00000);
    if flags & 0x10000 != 0 && (fx == 0 || flags & 0x8000 != 0)
        || flags & 0x4000 != 0 && (fx == 0 || flags & 0x2000 != 0)
    {
        return Ok(Err(INVALID));
    }
    if flags & 0x1000 != 0 {
        return Ok(Err(0x88760118));
    }
    if flags & (0x400 | 0x2000000) != 0 {
        if flags & 0x20000 != 0 || source.is_some() || fx == 0 {
            return Ok(Err(INVALID));
        }
        if flags & 0x400 != 0 {
            flags &= !0x2000000;
        }
        if surface.caps & 0x20000 != 0 && flags & 0x400 != 0
            || surface.caps & 0x20000 == 0 && flags & 0x2000000 != 0
        {
            return Ok(Err(INVALID));
        }
    } else {
        if flags & 0x20000 != 0 {
            if fx == 0 {
                return Ok(Err(INVALID));
            }
            match read_u32(p, fx.checked_add(8).ok_or(Error::InvalidRange)?)? {
                0xcc0020 => flags &= !0x20000,
                rop @ (0xff0662 | 0x42) => {
                    write_u32(
                        p,
                        fx.checked_add(80).ok_or(Error::InvalidRange)?,
                        if rop == 0xff0662 { u32::MAX } else { 0 },
                    )?;
                    flags = (flags & !0x20000) | 0x400;
                }
                _ => return Ok(Err(0x88760118)),
            }
        }
        if source.is_none() {
            return Ok(Err(INVALID));
        }
    }
    if flags & 0x8000 != 0 && source.is_none_or(|s| s.source_key.is_none())
        || flags & 0x2000 != 0 && surface.destination_key.is_none()
    {
        return Ok(Err(INVALID));
    }
    Ok(Ok(flags))
}
