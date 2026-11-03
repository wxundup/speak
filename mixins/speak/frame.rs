
/// Everything runs at 48 kHz mono, 16-bit signed little-endian PCM.
pub const SAMPLE_RATE: u32 = 48_000;
/// One frame is `(SAMPLE_RATE / 1000) * 20` samples: 20 ms, 960 samples.
pub const FRAME_SIZE: usize = (SAMPLE_RATE as usize / 1000) * 20;
pub const MAX_OPUS_PAYLOAD_SIZE: usize = 1275;
/// The floor of silence: levels below this are −127 dB.
pub const LOWEST_DB: f64 = -127.0;
/// The largest voice packet on the wire. Horizon's mod messages cap at
/// 30 000 bytes; the smaller bound still holds.
pub const MAX_VOICE_CHAT_PACKET_SIZE: usize = 2048;
/// A group name's length limit.
pub const MAX_GROUP_NAME_LENGTH: usize = 24;
/// A group password's length limit.
pub const MAX_GROUP_PASSWORD_LENGTH: usize = 256;

/// Little-endian 16-bit pairs to samples.
/// `None` when the input is odd-length.
pub fn bytes_to_shorts(bytes: &[u8]) -> Option<Vec<i16>> {
    if bytes.len() % 2 != 0 {
        return None;
    }
    Some(
        bytes
            .chunks_exact(2)
            .map(|pair| i16::from_le_bytes([pair[0], pair[1]]))
            .collect(),
    )
}

/// `AudioUtils.shortsToBytes`: samples to little-endian 16-bit pairs.
pub fn shorts_to_bytes(shorts: &[i16]) -> Vec<u8> {
    shorts.iter().flat_map(|s| s.to_le_bytes()).collect()
}

const FLOAT_SHORT_SCALE: f32 = i16::MAX as f32; // 32767
const FLOAT_CLIP: f32 = FLOAT_SHORT_SCALE - 1.0; // 32766

/// A float sample in [−1, 1] to a short, with an exact clamp: the upper
/// side stops one below full scale (`FLOAT_CLIP`), the lower side rests
/// at `−FLOAT_SHORT_SCALE`.
pub fn floats_to_shorts_normalized(audio: &[f32]) -> Vec<i16> {
    audio
        .iter()
        .map(|&f| {
            let x = f * FLOAT_SHORT_SCALE;
            let clamped = x.min(FLOAT_CLIP).max(-FLOAT_SHORT_SCALE);
            clamped as i16
        })
        .collect()
}

/// `AudioUtils.shortsToFloatsNormalized`: a short back to its normalized
/// float, scaling by `1 / 32767`.
pub fn shorts_to_floats_normalized(audio: &[i16]) -> Vec<f32> {
    audio
        .iter()
        .map(|&s| s as f32 * (1.0 / FLOAT_SHORT_SCALE))
        .collect()
}

pub fn stereo_floats_to_mono_shorts_normalized(stereo: &[f32]) -> Vec<i16> {
    stereo
        .chunks_exact(2)
        .map(|pair| {
            let x = (pair[0] + pair[1]) / 2.0 * FLOAT_SHORT_SCALE;
            let clamped = x.min(FLOAT_CLIP).max(-FLOAT_SHORT_SCALE);
            clamped as i16
        })
        .collect()
}

/// A clamped saturating float-to-short cast: the clamp to the 16-bit
/// rails happens before the cast, so ±2.0 lands on the rails, not on
/// truncated low bits.
pub fn floats_to_shorts(floats: &[f32]) -> Vec<i16> {
    floats
        .iter()
        .map(|&f| {
            let clamped = f.clamp(i16::MIN as f32, i16::MAX as f32);
            clamped as i16
        })
        .collect()
}

/// `AudioUtils.shortsToFloats`: shorts as their plain float values.
pub fn shorts_to_floats(shorts: &[i16]) -> Vec<f32> {
    shorts.iter().map(|&s| s as f32).collect()
}

/// The dB level of one peak sample (`AudioUtils.sampleDb`): the magnitude
/// against full scale (32768), `20 * log10`, clamped to `[−127, 0]`. Zero
/// is the floor, not −infinity.
pub fn sample_db(sample: i16) -> f64 {
    if sample == 0 {
        return LOWEST_DB;
    }
    let mag = sample.unsigned_abs();
    let norm = f64::from(mag) / 32768.0;
    let db = 20.0 * norm.log10();
    if !db.is_finite() {
        return LOWEST_DB;
    }
    db.clamp(LOWEST_DB, 0.0)
}

/// `AudioUtils.getHighestAudioLevel`: the frame's level is its loudest
/// sample's level.
pub fn get_highest_audio_level(samples: &[i16]) -> f64 {
    let highest = samples.iter().fold(0i16, |h, &s| h.max(s.unsigned_abs() as i16));
    sample_db(highest)
}

/// `AudioUtils.dbToLinear`: +6 dB is about 1.995, −6 dB about 0.501.
pub fn db_to_linear(db: f64) -> f64 {
    10.0f64.powf(db / 20.0)
}

/// `AudioUtils.linearToDb`: the gain of a multiplier, floored at
/// `LOWEST_DB` below 0.001.
pub fn linear_to_db(multiplier: f64) -> f64 {
    if multiplier < 0.001 {
        return LOWEST_DB;
    }
    20.0 * multiplier.log10()
}

pub fn db_sample(dbfs: f64) -> i32 {
    let value = 32768.0 * 10.0f64.powf(dbfs / 20.0);
    (value.round() as i64).clamp(0, 32768) as i32
}

pub fn is_above_threshold(samples: &[i16], threshold: f64) -> bool {
    get_highest_audio_level(samples) > threshold
}

/// Several frames summed sample by sample, saturating at the 16-bit
/// rails, into one `FRAME_SIZE` frame. `None` parts contribute silence.
pub fn combine_audio(parts: &[Option<Vec<i16>>]) -> Vec<i16> {
    let mut out = vec![0i16; FRAME_SIZE];
    for i in 0..FRAME_SIZE {
        let mut sample: i64 = 0;
        for part in parts {
            if let Some(audio) = part {
                if let Some(&s) = audio.get(i) {
                    sample += i64::from(s);
                }
            }
        }
        out[i] = sample.clamp(i64::from(i16::MIN), i64::from(i16::MAX)) as i16;
    }
    out
}

/// `AudioUtils.dbToPerc`: a dB level in `[-127, 0]` as a 0..1 meter
/// position.
pub fn db_to_perc(db: f64) -> f64 {
    let db = db.clamp(LOWEST_DB, 0.0);
    (db - LOWEST_DB) / -LOWEST_DB
}

/// A 0..1 meter position back to dB: `(1 - perc) * LOWEST_DB` with
/// `LOWEST_DB` negative, which is the same as `-(1 - perc) * 127`; the
/// exact form is kept.
pub fn perc_to_db(perc: f64) -> f64 {
    let perc = perc.clamp(0.0, 1.0);
    (1.0 - perc) * LOWEST_DB
}

/// `Utils.normalizeAngle`: an angle in degrees into `(-180, 180]`.
pub fn normalize_angle(angle: f32) -> f32 {
    let mut a = angle % 360.0;
    if a <= -180.0 {
        a += 360.0;
    } else if a > 180.0 {
        a -= 360.0;
    }
    a
}

/// `Utils.angle`: the angle from `a` to `b`, in degrees, as vanilla's
/// `Vec2` angle does — the signed angle between the two vectors measured
/// counterclockwise, computed from the cross and dot products.
pub fn angle_between(a: [f32; 2], b: [f32; 2]) -> f32 {
    let dot = a[0] * b[0] + a[1] * b[1];
    let cross = a[0] * b[1] - a[1] * b[0];
    cross.atan2(dot).to_degrees()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, eps: f64) -> bool {
        (a - b).abs() < eps
    }

    #[test]
    fn the_constants_match_simple_voice_chat() {
        assert_eq!(SAMPLE_RATE, 48_000);
        assert_eq!(FRAME_SIZE, 960);
        assert_eq!(MAX_OPUS_PAYLOAD_SIZE, 1275);
        assert_eq!(LOWEST_DB, -127.0);
        assert_eq!(MAX_VOICE_CHAT_PACKET_SIZE, 2048);
        assert_eq!(MAX_GROUP_NAME_LENGTH, 24);
        assert_eq!(MAX_GROUP_PASSWORD_LENGTH, 256);
    }

    #[test]
    fn sample_db_floors_and_clamps_exactly() {
        assert_eq!(sample_db(0), LOWEST_DB, "silence is the floor");
        assert!(
            close(sample_db(1), -90.3, 0.1),
            "one LSB against full scale: {}",
            sample_db(1)
        );
        // 32767 is one below full scale (32768, unreachable by an i16), so
        // its level is just under 0 dB.
        assert!(
            close(sample_db(32767), -0.000265, 1e-5),
            "one below full scale: {}",
            sample_db(32767)
        );
        assert!(
            close(sample_db(-32768), 0.0, 1e-9),
            "the negative rail IS full scale"
        );
    }

    #[test]
    fn the_frame_level_is_the_loudest_sample() {
        let frame = [0i16, 100, -3000, 5];
        assert!(close(
            get_highest_audio_level(&frame),
            sample_db(3000),
            1e-12
        ));
        assert_eq!(get_highest_audio_level(&[]), LOWEST_DB);
    }

    #[test]
    fn thresholds_are_strict() {
        let level = get_highest_audio_level(&[1000]);
        let threshold = sample_db(1000);
        assert!(!is_above_threshold(&[1000], threshold), "equal is not above");
        assert!(is_above_threshold(&[2000], threshold), "louder is above");
        assert!(close(level, sample_db(1000), 1e-12));
    }

    #[test]
    fn db_conversions_round_trip() {
        for db in [-127.0, -50.0, -20.0, -6.0, -0.5] {
            assert!(close(db_to_linear(db), 10.0f64.powf(db / 20.0), 1e-12));
            assert!(
                close(linear_to_db(db_to_linear(db)), db, 1e-6),
                "round trip at {db}"
            );
        }
        assert!(close(db_to_linear(-100.0), 1e-5, 1e-15));
        assert_eq!(linear_to_db(db_to_linear(-100.0)), LOWEST_DB);
        assert_eq!(linear_to_db(0.0005), LOWEST_DB, "below the floor");
        assert!(close(f64::from(db_sample(-5.0)), 18427.0, 0.5), "the AGC target");
        assert_eq!(db_sample(0.0), 32768);
        assert_eq!(db_sample(-1000.0), 0);
    }

    #[test]
    fn meters_round_trip() {
        for db in [-127.0, -60.0, -20.0, 0.0] {
            assert!(close(perc_to_db(db_to_perc(db)), db, 1e-9), "at {db}");
        }
        assert_eq!(db_to_perc(0.0), 1.0);
        assert_eq!(db_to_perc(-127.0), 0.0);
        assert_eq!(db_to_perc(50.0), 1.0, "clamped above");
        assert_eq!(perc_to_db(0.0), LOWEST_DB);
        assert_eq!(perc_to_db(1.0), 0.0);
    }

    #[test]
    fn float_short_conversion_clamps_to_the_rails() {
        // min(x, 32766) then max(.., -32767), cast to short.
        assert_eq!(floats_to_shorts_normalized(&[1.0])[0], 32766);
        assert_eq!(floats_to_shorts_normalized(&[-1.0])[0], -32767);
        assert_eq!(floats_to_shorts_normalized(&[2.0])[0], 32766);
        assert_eq!(floats_to_shorts_normalized(&[0.5])[0], (0.5 * 32767.0) as i16);
        // Round trip loses at most the one-LSB clamp.
        let back = shorts_to_floats_normalized(&floats_to_shorts_normalized(&[0.25]));
        assert!(close(f64::from(back[0]), 0.25, 0.001));
        // Stereo downmix: (L + R) / 2.
        let mono = stereo_floats_to_mono_shorts_normalized(&[1.0, -1.0, 0.5, 0.5]);
        assert_eq!(mono[0], 0, "(1 + -1) / 2 is silence");
        assert_eq!(mono[1], (0.5 * 32767.0) as i16);
    }

    #[test]
    fn plain_conversion_clamps_without_scaling() {
        // floatsToShorts only clamps to the 16-bit range; a float of
        // 2.0 is NOT scaled to full scale, it stays 2 (the normalized
        // variant above is the scaling one).
        assert_eq!(floats_to_shorts(&[2.0])[0], 2);
        assert_eq!(floats_to_shorts(&[-2.0])[0], -2);
        assert_eq!(floats_to_shorts(&[1e10])[0], i16::MAX, "clamped high");
        assert_eq!(floats_to_shorts(&[-1e10])[0], i16::MIN, "clamped low");
        assert_eq!(shorts_to_floats(&[-3])[0], -3.0);
    }

    #[test]
    fn byte_conversion_is_little_endian_and_odd_input_refuses() {
        let shorts = [-1i16, 256, -32768, 32767];
        let bytes = shorts_to_bytes(&shorts);
        assert_eq!(bytes, vec![0xFF, 0xFF, 0x00, 0x01, 0x00, 0x80, 0xFF, 0x7F]);
        assert_eq!(bytes_to_shorts(&bytes).as_deref(), Some(&shorts[..]));
        assert_eq!(bytes_to_shorts(&[1, 2, 3]), None, "odd length");
        assert_eq!(bytes_to_shorts(&[]).as_deref(), Some(&[][..]));
    }

    #[test]
    fn combining_sums_and_saturates() {
        let a = vec![100i16; FRAME_SIZE];
        let b = vec![200i16; FRAME_SIZE];
        let sum = combine_audio(&[Some(a.clone()), Some(b)]);
        assert_eq!(sum[0], 300);
        let loud = vec![i16::MAX; FRAME_SIZE];
        let clipped = combine_audio(&[Some(loud.clone()), Some(loud)]);
        assert_eq!(clipped[0], i16::MAX, "saturated at the rail");
        let low = vec![i16::MIN; FRAME_SIZE];
        let clipped = combine_audio(&[Some(low.clone()), Some(low)]);
        assert_eq!(clipped[0], i16::MIN);
        let with_none = combine_audio(&[None, Some(a.clone()), None]);
        assert_eq!(with_none[0], 100, "None parts are silence");
        let empty = combine_audio(&[]);
        assert!(empty.iter().all(|&s| s == 0));
    }

    #[test]
    fn angles_normalize_into_the_semicircle() {
        assert!(close(f64::from(normalize_angle(180.0)), 180.0, 1e-6));
        assert!(close(f64::from(normalize_angle(-180.0)), 180.0, 1e-6),
            "normalize maps -180 into the open end");
        assert!(close(f64::from(normalize_angle(360.0)), 0.0, 1e-6));
        assert!(close(f64::from(normalize_angle(-270.0)), 90.0, 1e-6));
        assert!(close(f64::from(normalize_angle(540.0)), 180.0, 1e-6));
    }

    #[test]
    fn the_angle_between_vectors_is_the_counterclockwise_one() {
        // East to north is +90° in a cross/dot sense.
        let east = [1.0, 0.0];
        let north = [0.0, 1.0];
        assert!(close(f64::from(angle_between(east, north)), 90.0, 1e-4));
        assert!(close(f64::from(angle_between(north, east)), -90.0, 1e-4));
        assert!(close(f64::from(angle_between(east, east)), 0.0, 1e-4));
        assert!(close(f64::from(angle_between(east, [-1.0, 0.0])), 180.0, 1e-4));
    }
}
