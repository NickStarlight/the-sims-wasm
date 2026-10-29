use super::*;
const TONE: &[u8] = include_bytes!("../../fixtures/stereo-tone.mp3");

impl MusicStream {
    fn rewind(&mut self) -> Result<()> {
        // Re-probe the same compressed allocation. This also resets gapless trim,
        // decoder reservoir, interpolation, and metadata consistently.
        *self = Self::open(self.encoded.clone())?;
        Ok(())
    }

    fn duration_seconds(&self) -> Option<f64> {
        self.duration
    }

    fn position_seconds(&self) -> f64 {
        self.duration
            .map_or(self.position, |duration| self.position.min(duration))
    }
}

fn render(block: usize, rate: u32) -> (Vec<f32>, Vec<f32>) {
    let mut stream = MusicStream::new(TONE.to_vec()).unwrap();
    let (mut l, mut r) = (Vec::new(), Vec::new());
    for _ in 0..1000 {
        if stream.finished() {
            break;
        }
        let mut left = vec![0.0; block];
        let mut right = vec![0.0; block];
        stream.mix(&mut left, &mut right, rate, 1.0, 1.0).unwrap();
        l.extend(left);
        r.extend(right);
        assert!(stream.samples.len() <= 2304);
    }
    assert!(stream.finished());
    assert!((0.24..0.30).contains(&stream.position_seconds()));
    (l, r)
}

#[test]
fn resampling_is_stereo_finite_and_independent_of_block_boundaries() {
    for rate in [22050, 44100, 48000, 96000] {
        let (a, b) = render(127, rate);
        let (c, d) = render(2048, rate);
        let n = a.len().min(c.len());
        assert_eq!(&a[..n], &c[..n]);
        assert_eq!(&b[..n], &d[..n]);
        assert!(a.iter().chain(&b).all(|x| x.is_finite()));
        assert!(a.iter().any(|x| x.abs() > 0.1));
        assert!(b.iter().any(|x| x.abs() > 0.05));
        assert_ne!(&a[..1000], &b[..1000]);
        assert!(a[n..]
            .iter()
            .chain(&b[n..])
            .chain(&c[n..])
            .chain(&d[n..])
            .all(|&x| x == 0.0));
    }
}

#[test]
fn rewind_resets_decoder_and_gain_is_additive() {
    let mut stream = MusicStream::new(TONE.to_vec()).unwrap();
    let mut left = [0.0; 4096];
    let mut right = [0.0; 4096];
    stream.mix(&mut left, &mut right, 48000, 1.0, 1.0).unwrap();
    assert!(stream.position_seconds() > 0.08);
    stream.rewind().unwrap();
    assert_eq!(stream.position_seconds(), 0.0);
    let mut half = [0.25; 4096];
    let mut muted = [0.25; 4096];
    stream.mix(&mut half, &mut muted, 48000, 0.5, 0.0).unwrap();
    for i in 0..4096 {
        assert_eq!(half[i], 0.25 + 0.5 * left[i]);
    }
    assert_eq!(muted, [0.25; 4096]);
    assert!(stream
        .duration_seconds()
        .is_some_and(|d| (0.24..0.30).contains(&d)));
}

#[test]
fn malformed_input_and_invalid_output_are_rejected() {
    assert!(MusicStream::new(Vec::new()).is_err());
    assert!(MusicStream::new(vec![0; 8192]).is_err());
    let mut stream = MusicStream::new(TONE.to_vec()).unwrap();
    assert!(stream
        .mix(&mut [0.0; 2], &mut [0.0; 1], 48000, 1.0, 1.0)
        .is_err());
    assert!(stream
        .mix(&mut [0.0; 2], &mut [0.0; 2], 0, 1.0, 1.0)
        .is_err());
    assert_eq!(stream.position_seconds(), 0.0);
}

#[test]
fn completion_is_silent_and_rewind_replays_from_the_start() {
    let mut stream = MusicStream::new(TONE.to_vec()).unwrap();
    let mut first = [0.0; 256];
    stream
        .mix(&mut first, &mut [0.0; 256], 48000, 1.0, 1.0)
        .unwrap();
    for _ in 0..20 {
        stream
            .mix(&mut [0.0; 4096], &mut [0.0; 4096], 48000, 1.0, 1.0)
            .unwrap();
        if stream.finished() {
            break;
        }
    }
    assert!(stream.finished());
    let mut left = [0.25; 256];
    let mut right = [0.5; 256];
    stream.mix(&mut left, &mut right, 48000, 1.0, 1.0).unwrap();
    assert_eq!(left, [0.25; 256]);
    assert_eq!(right, [0.5; 256]);
    stream.rewind().unwrap();
    assert!(!stream.finished());
    left.fill(0.0);
    stream.mix(&mut left, &mut right, 48000, 1.0, 1.0).unwrap();
    assert_eq!(left, first);
}
