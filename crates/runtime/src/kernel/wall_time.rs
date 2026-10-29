//! Legacy FILETIME/SYSTEMTIME conversion, using the time crate for calendar arithmetic.
use super::*;
impl Kernel {
    pub fn set_wall_time(&mut self, ms: f64) -> Result<()> {
        if !ms.is_finite() || ms.abs() > 8.64e15 {
            return Err(Error::InvalidRange);
        }
        self.wall_time = Some(ms as i64);
        Ok(())
    }
    pub(super) fn wall_time_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        call: Call,
        a: &[u32],
    ) -> Result<u32> {
        if call == Call::GetSystemTimeAsFileTime {
            let ms = self
                .wall_time
                .take()
                .ok_or_else(|| Error::Host("wall-clock API requires a fresh host sample".into()))?;
            let ticks = u64::try_from((i128::from(ms) + 11_644_473_600_000) * 10_000)
                .map_err(|_| Error::InvalidRange)?;
            p.write(a[0], &ticks.to_le_bytes())?;
            return Ok(0);
        }
        if call == Call::FileTimeToLocalFileTime {
            if a[0] != 0 && a[1] != 0 {
                let mut b = [0; 8];
                p.read(a[0], &mut b)?;
                p.write(a[1], &b)?;
            }
            return Ok(1);
        }
        let (ms, out, result) = if call == Call::FileTimeToSystemTime {
            if a[0] == 0 || a[1] == 0 {
                return Ok(1);
            }
            let mut b = [0; 8];
            p.read(a[0], &mut b)?;
            (
                (u64::from_le_bytes(b) / 10000) as i64 - 11644473600000,
                a[1],
                1,
            )
        } else {
            (
                self.wall_time.take().ok_or_else(|| {
                    Error::Host("wall-clock API requires a fresh host sample".into())
                })?,
                a[0],
                0,
            )
        };
        if out == 0 {
            return Ok(result);
        }
        let date = time::OffsetDateTime::from_unix_timestamp_nanos(i128::from(ms) * 1_000_000)
            .map_err(|_| Error::InvalidRange)?;
        let fields = [
            date.year() as u16,
            date.month() as u16,
            date.weekday().number_days_from_sunday() as u16,
            date.day() as u16,
            date.hour() as u16,
            date.minute() as u16,
            date.second() as u16,
            date.millisecond(),
        ];
        let bytes: Vec<_> = fields.iter().flat_map(|v| v.to_le_bytes()).collect();
        p.write(out, &bytes)?;
        Ok(result)
    }
}
