use super::frame::db_to_linear;

/// The manual gain range in dB (`VolumeManager.MIN_GAIN`/`MAX_GAIN`).
pub const MIN_GAIN: f64 = -40.0;
pub const MAX_GAIN: f64 = 24.0;

/// The largest multiplication the limiter allows against full scale:
/// one below `i16::MAX` (`VolumeManager.MAX_AMPLIFICATION` = 32766).
const MAX_AMPLIFICATION: f64 = 32766.0;

/// How many frames of peak history the limiter looks back over.
const HISTORY: usize = 50;

/// The gain manager, holding the last 50 frames' peak multipliers.
pub struct VolumeManager {
    /// Each frame's peak multiplier, −1 for "not set yet"; the entries
    /// are skipped while negative.
    max_volumes: [f64; HISTORY],
    index: usize,
}

impl VolumeManager {
    pub fn new() -> Self {
        Self {
            max_volumes: [-1.0; HISTORY],
            index: 0,
        }
    }

    /// Changes a frame's volume in place (`VolumeManager.adjustVolume`):
    /// the gain in dB becomes a multiplier, capped per frame by what its
    /// peak allows without clipping, and the applied multiplier is the
    /// smallest of the whole look-back window.
    pub fn adjust_volume(&mut self, audio: &mut [i16], gain_db: f64) {
        let max_multiplier = if gain_db <= MIN_GAIN {
            0.0
        } else {
            db_to_linear(gain_db)
        };
        self.max_volumes[self.index] = Self::maximum_multiplier(audio, max_multiplier);
        self.index = (self.index + 1) % self.max_volumes.len();
        // The smallest non-negative entry in the history (−1 skips).
        let mut min = -1.0;
        for &mul in &self.max_volumes {
            if mul < 0.0 {
                continue;
            }
            if min < 0.0 || mul < min {
                min = mul;
            }
        }
        let max_volume = min.min(max_multiplier);
        for s in audio.iter_mut() {
            *s = (f64::from(*s) * max_volume) as i16;
        }
    }

    /// The multiplier this frame's peak allows (`getMaximumMultiplier`):
    /// the gain's multiplier, or whatever keeps the loudest sample at or
    /// under `MAX_AMPLIFICATION`. A silent frame caps nothing.
    fn maximum_multiplier(audio: &[i16], multiplier: f64) -> f64 {
        // The magnitude of the most negative sample needs the
        // two's-complement trick (|−32768| = 32768 overflows a short, so it
        // adds −1 first); `unsigned_abs` is the same number.
        let peak = audio.iter().fold(0u16, |p, &s| p.max(s.unsigned_abs()));
        if peak == 0 {
            return multiplier;
        }
        multiplier.min(MAX_AMPLIFICATION / f64::from(peak))
    }
}

impl Default for VolumeManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_constants_match_simple_voice_chat() {
        assert_eq!(MIN_GAIN, -40.0);
        assert_eq!(MAX_GAIN, 24.0);
        assert_eq!(MAX_AMPLIFICATION, 32766.0);
    }

    #[test]
    fn silence_at_the_floor_gain() {
        let mut audio = vec![1000i16; 960];
        let mut vm = VolumeManager::new();
        vm.adjust_volume(&mut audio, -40.0);
        assert!(audio.iter().all(|&s| s == 0), "gain <= MIN_GAIN silences");
        vm.adjust_volume(&mut audio, -100.0);
        assert!(audio.iter().all(|&s| s == 0));
    }

    #[test]
    fn quiet_frames_pass_through_at_the_requested_gain() {
        let mut vm = VolumeManager::new();
        let mut audio = vec![100i16; 960];
        vm.adjust_volume(&mut audio, 0.0);
        // 0 dB = multiplier 1.0; the frame's peak (100) allows 327.66, so
        // the full multiplier applies.
        assert!(audio.iter().all(|&s| s == 100));
        // A fresh manager for a different gain: the look-back limiter holds
        // the previous frame's multiplier, so raising the gain takes the
        // new value only after the window drains (that is the limiter).
        let mut vm = VolumeManager::new();
        let mut audio = vec![100i16; 960];
        vm.adjust_volume(&mut audio, 6.0);
        let expected = (100.0 * 10.0f64.powf(6.0 / 20.0)) as i16;
        assert_eq!(audio[0], expected, "+6 dB roughly doubles");
    }

    #[test]
    fn a_loud_frame_limits_itself_and_the_history() {
        let mut vm = VolumeManager::new();
        // A near-rail frame with +24 dB: the limiter caps it at 32766.
        let mut audio = vec![32000i16; 960];
        vm.adjust_volume(&mut audio, 24.0);
        assert_eq!(audio[0], 32766, "the peak is held at the amplification cap");
        // The next quiet frame is STILL limited (the history holds the loud
        // frame's multiplier for 50 frames).
        let mut audio = vec![1000i16; 960];
        vm.adjust_volume(&mut audio, 24.0);
        let expected = (1000.0 * (32766.0 / 32000.0)) as i16;
        assert_eq!(audio[0], expected, "the look-back limiter holds");
        // After 50 quiet frames the limiter has released.
        for _ in 0..50 {
            let mut audio = vec![1i16; 960];
            vm.adjust_volume(&mut audio, 24.0);
        }
        let mut audio = vec![1000i16; 960];
        vm.adjust_volume(&mut audio, 24.0);
        let expected = (1000.0 * 10.0f64.powf(24.0 / 20.0)) as i16;
        assert_eq!(audio[0], expected.min(32766), "released after the window");
    }

    #[test]
    fn the_history_is_a_ring_of_fifty() {
        let mut vm = VolumeManager::new();
        for i in 0..60 {
            let mut audio = vec![if i == 0 { 32000 } else { 1 }; 960];
            vm.adjust_volume(&mut audio, 0.0);
        }
        // The loud frame left the ring 10 frames ago; the gain is full again.
        let mut audio = vec![1000i16; 960];
        vm.adjust_volume(&mut audio, 0.0);
        assert_eq!(audio[0], 1000);
    }

    #[test]
    fn negative_samples_meet_the_same_cap() {
        let mut vm = VolumeManager::new();
        let mut audio = vec![i16::MIN; 960];
        vm.adjust_volume(&mut audio, 24.0);
        // |−32768| = 32768 > 32766, so the cap scales it down; the
        // magnitude trick (|s + 1| for the most negative) gives 32767, and
        // the multiplier is 32766/32767.
        let expected = (-32768.0 * (32766.0 / 32767.0)) as i16;
        assert_eq!(audio[0], expected, "the most negative sample is capped");
    }
}
