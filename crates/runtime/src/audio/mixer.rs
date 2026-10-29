//! Original linear resampling and stereo gain/pan algorithm; only the output transport changed.
use super::Buffer;
pub(super) fn mix(b: &mut Buffer, left: &mut [f32], right: &mut [f32], rate: f64) {
    let length = b.bytes / b.format.align;
    let looping = b.looping;
    let step = b.freq as f64 / rate;
    let gain = libm::pow(10.0, b.volume as f64 / 2000.0);
    let pan = (b.pan as f64 / 10000.0).clamp(-1.0, 1.0);
    let gl = gain * if pan < 0.0 { 1.0 } else { 1.0 - pan };
    let gr = gain * if pan > 0.0 { 1.0 } else { 1.0 + pan };
    let mut position = b.read;
    for i in 0..left.len() {
        if position >= length as f64 {
            if !looping {
                break;
            }
            position %= length as f64;
        }
        let frame = libm::floor(position) as u64;
        let next = if frame + 1 < u64::from(length) {
            frame + 1
        } else if looping {
            0
        } else {
            frame
        };
        let index = frame as usize * b.format.channels as usize;
        let next_index = next as usize * b.format.channels as usize;
        let t = position - frame as f64;
        let l = b.pcm[index] as f64 + t * (b.pcm[next_index] as f64 - b.pcm[index] as f64);
        let r = if b.format.channels > 1 {
            b.pcm[index + 1] as f64 + t * (b.pcm[next_index + 1] as f64 - b.pcm[index + 1] as f64)
        } else {
            l
        };
        left[i] = (left[i] as f64 + gl * l) as f32;
        right[i] = (right[i] as f64 + gr * r) as f32;
        position += step;
    }
    b.read = position;
}
