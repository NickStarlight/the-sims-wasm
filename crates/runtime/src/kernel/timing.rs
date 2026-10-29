//! One host monotonic timeline drives performance counters and wrapping millisecond ticks.
use super::*;

const FREQUENCY: u64 = 1_000_000;

#[derive(Default)]
pub(super) struct Clock {
    last_ms: Option<f64>,
    sample: Option<u64>,
}

impl Kernel {
    pub(super) fn take_monotonic_time(&mut self) -> Result<f64> {
        self.clock
            .sample
            .take()
            .map(|sample| sample as f64 / 1000.0)
            .ok_or_else(|| Error::Host("clock API requires a fresh monotonic sample".into()))
    }

    pub(super) fn window_time(&mut self) -> Result<u32> {
        self.clock
            .sample
            .take()
            .map(|value| (value / 1000) as u32)
            .ok_or_else(|| Error::Host("message pump requires a fresh monotonic sample".into()))
    }

    /// The virtual process uses UTC without daylight-saving transitions, independently of the host locale.
    pub(super) fn time_zone<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        address: u32,
    ) -> Result<u32> {
        if address == 0 {
            self.fail(87);
            return Ok(u32::MAX); // TIME_ZONE_ID_INVALID
        }
        let mut information = [0u8; 172];
        for offset in [4, 88] {
            // StandardName and DaylightName: WCHAR[32].
            information[offset..offset + 6].copy_from_slice(&[b'U', 0, b'T', 0, b'C', 0]);
        }
        process.write(address, &information)?;
        Ok(0) // TIME_ZONE_ID_UNKNOWN: no daylight-saving transitions.
    }

    /// Supply a fresh host monotonic sample for one clock-reading API call.
    pub fn set_monotonic_time(&mut self, milliseconds: f64) -> Result<()> {
        if !milliseconds.is_finite()
            || milliseconds < 0.0
            || milliseconds * 1000.0 >= i64::MAX as f64
            || self.clock.last_ms.is_some_and(|last| milliseconds < last)
        {
            return Err(Error::Host(
                "invalid or decreasing monotonic clock sample".into(),
            ));
        }
        self.clock.last_ms = Some(milliseconds);
        self.clock.sample = Some((milliseconds * 1000.0) as u64);
        Ok(())
    }

    pub(super) fn time_call<M: GuestMemory>(
        &mut self,
        process: &mut Process<M>,
        call: Call,
        args: &[u32],
    ) -> Result<u32> {
        let value = if call == Call::QueryPerformanceFrequency {
            FREQUENCY
        } else {
            self.clock
                .sample
                .take()
                .ok_or_else(|| Error::Host("clock API requires a fresh monotonic sample".into()))?
        };
        if matches!(call, Call::GetTickCount | Call::TimeGetTime) {
            return Ok((value / 1000) as u32);
        }
        if args[0] == 0 {
            return Ok(self.fail(87));
        }
        process.write(args[0], &value.to_le_bytes())?;
        Ok(1)
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/timing.rs"]
mod tests;
