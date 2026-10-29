//! Playback state ported from the original DirectSound runtime.
use super::PcmFormat as Format;
#[derive(Clone)]
pub(super) struct Buffer {
    pub(super) address: u32,
    pub(super) bytes: u32,
    pub(super) flags: u32,
    pub(super) primary: bool,
    pub(super) format: Format,
    pub(super) freq: u32,
    pub(super) volume: i32,
    pub(super) pan: i32,
    pub(super) playing: bool,
    pub(super) looping: bool,
    pub(super) started: f64,
    pub(super) base: u32,
    pub(super) write: u32,
    pub(super) pcm: Vec<f32>,
    pub(super) read: f64,
}
impl Buffer {
    pub(super) fn average(&self) -> u32 {
        self.format.rate * self.format.channels * (self.format.bits >> 3)
    }
    pub(super) fn playing(&self, now: f64) -> bool {
        self.playing
            && (self.looping
                || (now - self.started) * (self.average() as f64) < 1000.0 * self.bytes as f64)
    }
    pub(super) fn position(&self, now: f64) -> u32 {
        let elapsed = if self.playing {
            libm::floor((now - self.started) * self.average() as f64 / 1000.0) as u64
        } else {
            0
        };
        ((self.base as u64 + elapsed) % self.bytes as u64) as u32
    }
    pub(super) fn reset(&mut self) {
        self.playing = false;
        self.looping = false;
        self.started = 0.0;
        self.base = 0;
        self.write = 0;
        self.read = 0.0;
    }
}
