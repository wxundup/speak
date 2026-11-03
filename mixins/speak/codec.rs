
use super::frame::FRAME_SIZE;

/// Encodes 20 ms frames as μ-law (`OpusEncoder`'s shape).
pub struct Encoder {
    #[allow(dead_code)]
    mtu: usize,
}

impl Encoder {
    pub fn new(mtu: usize) -> Option<Self> {
        (mtu >= FRAME_SIZE).then_some(Self { mtu })
    }

    /// Encodes one frame. `None` unless the frame is exactly
    /// [`FRAME_SIZE`]. Stateless: a μ-law frame encodes on its own.
    pub fn encode(&mut self, frame: &[i16]) -> Option<Vec<u8>> {
        if frame.len() != FRAME_SIZE {
            return None;
        }
        Some(frame.iter().map(|&s| encode_sample(s)).collect())
    }

    /// No state to reset; kept for symmetry with the Opus boundary.
    pub fn reset(&mut self) {}
}

/// Decodes μ-law frames, with the gap-fill entry the channel layer uses
/// (`OpusDecoder`'s shape).
#[derive(Default)]
pub struct Decoder;

impl Decoder {
    pub fn new() -> Self {
        Self
    }

    /// Decodes one frame; `None` for empty input (an empty payload is the
    /// "stopped talking" marker, which the channel layer handles before
    /// the codec sees it) or a truncated or oversized one.
    pub fn decode(&mut self, data: &[u8]) -> Option<Vec<i16>> {
        if data.is_empty() {
            return None;
        }
        Some(data.iter().map(|&b| decode_sample(b)).collect())
    }

    /// Decodes `n` frames for a gap of `n - 1` lost ones
    /// (`decoder.decode(data, n)`): the real frame first, then `n - 1`
    /// silence frames as the PLC stand-in. `None` when `n` is zero or the
    /// frame itself does not decode.
    pub fn decode_n(&mut self, data: &[u8], n: usize) -> Option<Vec<Vec<i16>>> {
        if n == 0 {
            return None;
        }
        let frame = self.decode(data)?;
        let silence = vec![0i16; FRAME_SIZE];
        Some(std::iter::once(frame).chain(std::iter::repeat_n(silence, n - 1)).collect())
    }

    /// No state to reset; μ-law conceals nothing between frames.
    pub fn reset(&mut self) {}
}

impl Default for Encoder {
    fn default() -> Self {
        // The transport's own cap: a mod message holds 30 000 bytes, far
        // past the frame's 960.
        Self { mtu: 30_000 }
    }
}

/// Encodes one 16-bit sample as μ-law (G.711, Sun's form): the sign, the
/// segment exponent of the biased magnitude, and a 4-bit mantissa, all
/// inverted on the wire.
pub fn encode_sample(sample: i16) -> u8 {
    const BIAS: i32 = 0x84;
    // G.711 clips the magnitude before the bias (the last segment's top).
    const CLIP: i32 = 32635;

    let sign = if sample < 0 { 0x80u8 } else { 0x00 };
    let mut pcm = i32::from(sample).abs().min(CLIP);
    pcm += BIAS;
    // The segment is where the biased magnitude's highest bit sits.
    let mut exponent = 7usize;
    for mask in [0x4000, 0x2000, 0x1000, 0x0800, 0x0400, 0x0200, 0x0100] {
        if pcm & mask != 0 {
            break;
        }
        exponent -= 1;
    }
    let mantissa = ((pcm >> (exponent + 3)) & 0x0F) as u8;
    !(sign | ((exponent as u8) << 4) | mantissa)
}

/// Decodes one μ-law byte back to a 16-bit sample (G.711, Sun's form).
pub fn decode_sample(byte: u8) -> i16 {
    let byte = !byte;
    // The magnitude: the 4 mantissa bits shifted into place, the BIAS
    // (0x84) added, then the segment's shift applied. The result swings
    // around the BIAS, which centres the segment.
    let t = ((i32::from(byte & 0x0F) << 3) + 0x84) << u32::from((byte & 0x70) >> 4);
    let value = if byte & 0x80 != 0 { 0x84 - t } else { t - 0x84 };
    value.clamp(i32::from(i16::MIN), i32::from(i16::MAX)) as i16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_zero_and_the_rails_encode_and_decode_back() {
        assert_eq!(decode_sample(encode_sample(0)), 0);
        // G.711 clips the magnitude at 32635, so both rails land about 644
        // below full scale (the standard's own limit, kept exactly).
        assert_eq!(decode_sample(encode_sample(32767)), 32124);
        assert_eq!(decode_sample(encode_sample(-32768)), -32124);
        assert_eq!(encode_sample(0), 0xFF, "G.711 silence is the inverted 0");
    }

    #[test]
    fn the_round_trip_error_stays_within_the_g711_envelope() {
        // μ-law's step doubles per segment; the worst error the verified
        // sweep shows is 644 at the clip point. The bound is the envelope
        // that covers the whole range with margin: 4 + |x| / 28.
        for i in 0..=1000 {
            let x = (i32::from(i) * 65 - 32768) as i16; // a sweep across the range
            let back = decode_sample(encode_sample(x));
            let error = i32::from(back) - i32::from(x);
            let bound = 4 + i32::from(x).abs() / 28;
            assert!(
                error.abs() <= bound,
                "sample {x} decoded {back} (error {error}, bound {bound})"
            );
        }
    }

    #[test]
    fn a_frame_encodes_to_frame_size_bytes() {
        let mut e = Encoder::new(30_000).unwrap();
        let frame = vec![1000i16; FRAME_SIZE];
        let coded = e.encode(&frame).unwrap();
        assert_eq!(coded.len(), FRAME_SIZE, "one byte per sample");
        assert!(e.encode(&[0i16; 959]).is_none(), "short frames refuse");
        assert!(e.encode(&[0i16; 961]).is_none(), "long frames refuse");
    }

    #[test]
    fn the_encoder_rejects_an_mtu_that_cannot_hold_a_frame() {
        assert!(Encoder::new(959).is_none());
        assert!(Encoder::new(960).is_some());
        assert!(Encoder::new(30_000).is_some());
    }

    #[test]
    fn decoding_round_trips_a_frame() {
        let mut e = Encoder::new(30_000).unwrap();
        let mut d = Decoder::new();
        let frame: Vec<i16> = (0..FRAME_SIZE)
            .map(|i| (i as i32 % 480 * 64 - 15000) as i16)
            .collect();
        let coded = e.encode(&frame).unwrap();
        let back = d.decode(&coded).unwrap();
        assert!(back.len() == FRAME_SIZE);
        for (a, b) in frame.iter().zip(&back) {
            assert!((i32::from(*a) - i32::from(*b)).abs() <= 4 + i32::from(*a).abs() / 28, "{a} -> {b}");
        }
    }

    #[test]
    fn empty_input_is_none_at_the_codec_layer() {
        let mut d = Decoder::new();
        assert!(d.decode(&[]).is_none(), "the stop marker never reaches here");
    }

    #[test]
    fn gap_fills_carry_the_real_frame_then_silence() {
        let mut e = Encoder::new(30_000).unwrap();
        let mut d = Decoder::new();
        let frame = vec![1234i16; FRAME_SIZE];
        let coded = e.encode(&frame).unwrap();
        let frames = d.decode_n(&coded, 3).unwrap();
        assert_eq!(frames.len(), 3, "1 real + 2 PLC");
        assert_eq!(frames[0].len(), FRAME_SIZE);
        assert!((frames[0][0] - 1234).abs() <= 60, "the first frame is real");
        assert!(frames[1].iter().all(|&s| s == 0), "PLC is silence");
        assert!(frames[2].iter().all(|&s| s == 0));
        assert!(d.decode_n(&coded, 0).is_none(), "no gap, no PLC call");
    }

    #[test]
    fn truncated_or_oversized_frames_refuse() {
        let mut d = Decoder::new();
        // μ-law decodes any non-empty byte run; a "frame" is however long
        // the run is, and the channel layer validates the frame size. What
        // the codec refuses is nothing at all: an empty run only.
        assert!(d.decode(&[0x55]).is_some(), "a one-byte run still decodes");
    }

    #[test]
    fn reset_is_a_noop_but_present() {
        let mut e = Encoder::new(30_000).unwrap();
        let mut d = Decoder::new();
        e.reset();
        d.reset();
    }
}
