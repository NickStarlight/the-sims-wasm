//! Incremental MP3 decoding. Retain compressed bytes and one PCM packet, never a
//! decoded song. Playback state and DirectShow controls belong to the caller.
use crate::{Error, Result};
use std::{io::Cursor, sync::Arc};
use symphonia::core::{
    codecs::audio::AudioDecoder,
    errors::Error as DecodeError,
    formats::{probe::Hint, FormatReader, TrackType},
    io::MediaSourceStream,
};

pub(crate) struct MusicStream {
    #[cfg(test)]
    encoded: Arc<[u8]>,
    format: Box<dyn FormatReader>,
    decoder: Box<dyn AudioDecoder>,
    track: u32,
    rate: u32,
    channels: usize,
    samples: Vec<f32>,
    cursor: usize,
    eof: bool,
    current: Option<[f32; 2]>,
    next: Option<[f32; 2]>,
    fraction: f64,
    #[cfg(test)]
    position: f64,
    #[cfg(test)]
    duration: Option<f64>,
}

impl MusicStream {
    pub(crate) fn new(bytes: Vec<u8>) -> Result<Self> {
        Self::open(Arc::from(bytes))
    }

    fn open(encoded: Arc<[u8]>) -> Result<Self> {
        let source =
            MediaSourceStream::new(Box::new(Cursor::new(encoded.clone())), Default::default());
        let mut hint = Hint::new();
        hint.with_extension("mp3");
        let format = symphonia::default::get_probe()
            .probe(&hint, source, Default::default(), Default::default())
            .map_err(decode_error)?;
        let track = format
            .default_track(TrackType::Audio)
            .ok_or_else(|| Error::Host("MP3 has no audio track".into()))?;
        let params = track
            .codec_params
            .as_ref()
            .and_then(|p| p.audio())
            .ok_or_else(|| Error::Host("MP3 has no audio parameters".into()))?;
        let decoder = symphonia::default::get_codecs()
            .make_audio_decoder(params, &Default::default())
            .map_err(decode_error)?;
        #[cfg(test)]
        let duration = track
            .time_base
            .zip(track.duration)
            .and_then(|(base, duration)| base.calc_duration(duration))
            .map(|time| time.as_secs_f64())
            .or_else(|| {
                track
                    .num_frames
                    .zip(params.sample_rate)
                    .map(|(frames, rate)| frames as f64 / f64::from(rate))
            });
        let track_id = track.id;
        let mut stream = Self {
            #[cfg(test)]
            encoded,
            format,
            decoder,
            track: track_id,
            rate: 0,
            channels: 0,
            samples: Vec::new(),
            cursor: 0,
            eof: false,
            current: None,
            next: None,
            fraction: 0.0,
            #[cfg(test)]
            position: 0.0,
            #[cfg(test)]
            duration,
        };
        stream.current = stream.read_frame()?;
        if stream.current.is_none() {
            return Err(Error::Host("MP3 contains no decodable audio".into()));
        }
        stream.next = stream.read_frame()?;
        Ok(stream)
    }

    /// Adds at most one worklet block to the caller's stereo mix. Interpolation
    /// state crosses packet/block boundaries, so output is independent of block size.
    pub(crate) fn mix(
        &mut self,
        left: &mut [f32],
        right: &mut [f32],
        output_rate: u32,
        gain_left: f32,
        gain_right: f32,
    ) -> Result<()> {
        if left.len() != right.len()
            || left.len() > 4096
            || !(8000..=192000).contains(&output_rate)
            || !gain_left.is_finite()
            || !gain_right.is_finite()
        {
            return Err(Error::InvalidRange);
        }
        let step = f64::from(self.rate) / f64::from(output_rate);
        for (left, right) in left.iter_mut().zip(right) {
            let Some(a) = self.current else { break };
            let b = self.next.unwrap_or(a);
            let t = self.fraction as f32;
            *left += gain_left * (a[0] + t * (b[0] - a[0]));
            *right += gain_right * (a[1] + t * (b[1] - a[1]));
            #[cfg(test)]
            {
                self.position += 1.0 / f64::from(output_rate);
            }
            self.fraction += step;
            while self.fraction >= 1.0 {
                self.fraction -= 1.0;
                self.current = self.next;
                if self.current.is_none() {
                    self.fraction = 0.0;
                    break;
                }
                self.next = self.read_frame()?;
            }
        }
        Ok(())
    }

    pub(crate) fn finished(&self) -> bool {
        self.current.is_none()
    }

    fn read_frame(&mut self) -> Result<Option<[f32; 2]>> {
        if self.cursor == self.samples.len() && !self.eof {
            self.decode_packet()?;
        }
        if self.cursor == self.samples.len() {
            return Ok(None);
        }
        let left = self.samples[self.cursor];
        let right = self.samples[self.cursor + self.channels - 1];
        self.cursor += self.channels;
        Ok(Some([left, right]))
    }

    fn decode_packet(&mut self) -> Result<()> {
        self.samples.clear();
        self.cursor = 0;
        // Corrupt packets and other tracks cannot produce an unbounded retry loop
        // inside one audio callback. Normal MP3 packets contain <=1152 frames.
        for _ in 0..64 {
            let Some(packet) = self.format.next_packet().map_err(decode_error)? else {
                self.eof = true;
                return Ok(());
            };
            if packet.track_id != self.track {
                continue;
            }
            let audio = match self.decoder.decode(&packet) {
                Ok(audio) => audio,
                Err(DecodeError::DecodeError(_)) => continue,
                Err(error) => return Err(decode_error(error)),
            };
            let rate = audio.spec().rate();
            let channels = audio.spec().channels().count();
            if rate == 0
                || !matches!(channels, 1 | 2)
                || audio.samples_interleaved() > 2304
                || (self.rate != 0 && (self.rate != rate || self.channels != channels))
            {
                return Err(Error::Host("unsupported MP3 audio format change".into()));
            }
            self.rate = rate;
            self.channels = channels;
            self.samples.resize(audio.samples_interleaved(), 0.0);
            audio.copy_to_slice_interleaved(&mut self.samples);
            if !self.samples.is_empty() {
                return Ok(());
            }
        }
        Err(Error::Host("too many undecodable MP3 packets".into()))
    }
}

fn decode_error(error: DecodeError) -> Error {
    Error::Host(format!("MP3 decoding failed: {error}"))
}

#[cfg(test)]
#[path = "../../tests/unit/audio/music.rs"]
mod tests;
