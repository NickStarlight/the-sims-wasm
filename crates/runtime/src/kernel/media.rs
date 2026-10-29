//! WinMM import adapter around the ported owned media state.
use super::*;
impl Kernel {
    pub fn next_media_timer(&mut self, now_ms: f64) -> Result<Option<crate::MediaCallback>> {
        if !now_ms.is_finite() || now_ms < 0.0 || now_ms >= u64::MAX as f64 {
            return Err(Error::InvalidRange);
        }
        Ok(self.media.next_timer(now_ms as u64 as u32))
    }
    pub(super) fn media_call<M: GuestMemory>(
        &mut self,
        p: &mut Process<M>,
        call: Call,
        name: &str,
        a: &[u32],
    ) -> Result<u32> {
        match call {
            // Preserve the original unsupported wave/MIDI device contracts.
            Call::WaveOutGetNumDevs | Call::MidiOutGetNumDevs => Ok(0),
            Call::WaveOutGetDevCapsA | Call::MidiOutGetDevCapsA => Ok(2),
            Call::WaveOutOpen | Call::MidiOutOpen | Call::MidiStreamOpen => Ok(6),
            Call::WaveOutClose
            | Call::WaveOutPrepareHeader
            | Call::WaveOutUnprepareHeader
            | Call::WaveOutWrite
            | Call::WaveOutPause
            | Call::WaveOutRestart
            | Call::WaveOutReset
            | Call::WaveOutGetPosition
            | Call::WaveOutSetVolume
            | Call::WaveOutGetVolume
            | Call::MidiOutClose
            | Call::MidiOutShortMsg
            | Call::MidiStreamClose
            | Call::MidiStreamOut
            | Call::MidiStreamPause
            | Call::MidiStreamRestart
            | Call::MidiStreamStop
            | Call::MidiStreamSetPosition => Ok(5),

            Call::TimeGetDevCaps => {
                if a[0] == 0 || a[1] < 8 {
                    return Ok(97); // TIMERR_NOCANDO
                }
                p.write(a[0], &[1u32.to_le_bytes(), 65535u32.to_le_bytes()].concat())?;
                Ok(0)
            }
            Call::TimeBeginPeriod | Call::TimeEndPeriod => Ok(crate::media::Media::period(a[0])),
            Call::TimeSetEvent => {
                let now = self.take_monotonic_time()? as u64 as u32;
                Ok(self.media.create_timer(a[0], a[2], a[3], a[4], now))
            }
            Call::TimeKillEvent => Ok(self.media.kill_timer(a[0])),
            Call::MmioOpenA => {
                if a[0] == 0 {
                    return Ok(0);
                }
                let bytes = p.read_c_string(a[0], 32768)?;
                let name: String = bytes
                    .into_iter()
                    .map(|byte| char::from_u32(u32::from(text::decode_byte(byte))).unwrap())
                    .collect();
                let bytes = self.files.file_bytes(&name)?;
                match bytes {
                    Some(bytes) => self.media.open(bytes),
                    None => Ok(0),
                }
            }
            _ => self.media.mmio(p, name, a),
        }
    }
}

#[cfg(test)]
#[path = "../../tests/unit/kernel/media.rs"]
mod tests;
