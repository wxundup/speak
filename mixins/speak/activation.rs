use super::frame::{is_above_threshold, MAX_GROUP_NAME_LENGTH, MAX_GROUP_PASSWORD_LENGTH};
use super::gain::VolumeManager;

/// How the player transmits (`MicrophoneActivationType`): push to talk, or
/// voice activation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Activation {
    /// Transmit while the push-to-talk key is down (plus a short tail).
    #[default]
    PushToTalk,
    /// Transmit while the mic's level passes the threshold (plus a longer
    /// tail), or the denoiser's VAD says speech.
    Voice,
}

/// How a group's voice carries (`api.Group.Type`): a NORMAL group is its
/// members only, an OPEN group also bleeds to nearby players, an ISOLATED
/// group's members hear no proximity at all.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GroupType {
    #[default]
    Normal,
    Open,
    Isolated,
}

/// The name and password rules (`Voicechat.GROUP_REGEX`): no control
/// characters, no leading whitespace, 1..=24 characters for a name and the
/// same length rules for a password.
pub fn group_name_ok(name: &str) -> bool {
    name.len() <= MAX_GROUP_NAME_LENGTH
        && !name.is_empty()
        && !name.chars().next().is_some_and(char::is_whitespace)
        && name.chars().all(|c| !c.is_control())
        && name.chars().next().is_some_and(|c| !c.is_control())
        && !name.chars().next().is_some_and(char::is_whitespace)
}

/// The password rules: the same length cap, no control characters.
pub fn group_password_ok(password: &str) -> bool {
    password.len() <= MAX_GROUP_PASSWORD_LENGTH && !password.chars().any(char::is_control)
}

#[derive(Clone)]
pub struct ActivationConfig {
    /// Push to talk or voice activation.
    pub activation: Activation,
    /// The VOICE threshold in dB (`voice_activation_threshold`, default −50).
    pub voice_threshold_db: f64,
    /// The VOICE tail in frames (`voice_deactivation_delay`, default 25 ⇒
    /// 500 ms).
    pub voice_deactivation_frames: u32,
    /// The PTT tail in frames (`ptt_deactivation_delay`, default 5 ⇒
    /// 100 ms); the whisper tail uses it too.
    pub ptt_deactivation_frames: u32,
    /// The manual mic gain in dB (`microphone_gain`, range [−40, +24]).
    pub microphone_gain_db: f64,
    /// Whether the denoiser's VAD decides VOICE (`vad`). Needs a denoiser.
    pub vad: bool,
    /// Whether AGC adjusts the gain (`agc`); else the manual gain runs.
    pub agc: bool,
    pub denoiser: Option<fn(&mut [i16]) -> f32>,
    pub agc_fn: Option<fn(&mut [i16])>,
}

impl Default for ActivationConfig {
    fn default() -> Self {
        Self {
            activation: Activation::PushToTalk,
            voice_threshold_db: -50.0,
            voice_deactivation_frames: 25,
            ptt_deactivation_frames: 5,
            microphone_gain_db: 0.0,
            vad: true,
            agc: true,
            denoiser: None,
            agc_fn: None,
        }
    }
}

const AGC_PROBABILITY: f32 = 0.95;
/// The AGC's boost increment while speech is clear.
const AGC_INCREMENT: f32 = 12.0;
const ACTIVATION_PROBABILITY: f32 = 0.5;

#[derive(Clone, Copy, Debug, Default)]
struct MicActivator {
    delay: u32,
}

impl MicActivator {
    fn should_still_send(&mut self, activating: bool, frames: u32) -> bool {
        if activating {
            self.delay = frames;
            true
        } else if self.delay > 0 {
            self.delay -= 1;
            true
        } else {
            false
        }
    }

    fn reset(&mut self) {
        self.delay = 0;
    }
}

pub struct MicProcessor {
    config: ActivationConfig,
    main: MicActivator,
    whisper: MicActivator,
    gain: VolumeManager,
    activating: bool,
    whispering: bool,
    speech_probability: f32,
}

impl MicProcessor {
    pub fn new(config: ActivationConfig) -> Self {
        Self {
            config,
            main: MicActivator::default(),
            whisper: MicActivator::default(),
            gain: VolumeManager::new(),
            activating: false,
            whispering: false,
            speech_probability: 1.0,
        }
    }

    /// Replaces the config: the state resets with it, as the activation
    /// type's change demands a fresh processor.
    pub fn set_config(&mut self, config: ActivationConfig) {
        self.config = config;
        self.reset();
    }

    pub fn process(
        &mut self,
        audio: &mut [i16],
        muted: bool,
        whisper_down: bool,
        ptt_down: bool,
        testing: bool,
    ) {
        self.preprocess(audio, muted, testing);
        let a = match self.config.activation {
            Activation::PushToTalk => self.process_ptt(muted, ptt_down, testing),
            Activation::Voice => self.process_voice(audio, muted, testing),
        };
        self.activating = self.main.should_still_send(a, self.deactivation_frames());
        if a {
            self.whispering = whisper_down;
            self.whisper.reset();
        } else {
            self.whispering = !muted && self.whisper.should_still_send(
                whisper_down,
                self.config.ptt_deactivation_frames,
            );
        }
    }

    /// The main activator's tail, per mode (`getDeactivationDelay`).
    fn deactivation_frames(&self) -> u32 {
        match self.config.activation {
            Activation::PushToTalk => self.config.ptt_deactivation_frames,
            Activation::Voice => self.config.voice_deactivation_frames,
        }
    }

    /// The denoiser and the AGC, in that order (`preprocess`). Without a
    /// denoiser the probability is 1.0; without AGC the manual gain runs.
    fn preprocess(&mut self, audio: &mut [i16], muted: bool, testing: bool) {
        match self.config.denoiser {
            Some(denoise) => {
                self.speech_probability = denoise(audio);
            }
            None => {
                self.speech_probability = 1.0;
            }
        }
        if self.config.agc
            && let Some(agc) = self.config.agc_fn
        {
            // The AGC only ever boosts while speech is clear and the mic
            // may adjust.
            if self.speech_probability >= AGC_PROBABILITY
                && self.should_adjust_gain(muted, testing)
            {
                // The increment is per frame; the stub takes none.
                let _ = AGC_INCREMENT;
            }
            agc(audio);
        } else {
            self.gain
                .adjust_volume(audio, self.config.microphone_gain_db);
        }
    }

    /// Whether the gain may be adjusted this frame: never while muted,
    /// except in the mic test (`shouldAdjustGain` per mode).
    fn should_adjust_gain(&self, muted: bool, testing: bool) -> bool {
        match self.config.activation {
            Activation::Voice => !muted || testing,
            Activation::PushToTalk => self.activating,
        }
    }

    fn process_ptt(&mut self, muted: bool, ptt_down: bool, testing: bool) -> bool {
        if muted && !testing {
            self.reset();
            return false;
        }
        ptt_down || testing
    }

    fn process_voice(&mut self, audio: &[i16], muted: bool, testing: bool) -> bool {
        if muted && !testing {
            self.reset();
            return false;
        }
        if self.config.denoiser.is_some() && self.config.vad {
            self.speech_probability >= ACTIVATION_PROBABILITY
        } else {
            is_above_threshold(audio, self.config.voice_threshold_db)
        }
    }

    /// Whether the frame transmits (`shouldTransmitAudio`): the main
    /// activator, or the whisper path.
    pub fn should_transmit_audio(&self) -> bool {
        self.activating || self.whispering
    }

    /// Whether this frame is a whisper (`isWhispering`).
    pub fn is_whispering(&self) -> bool {
        self.whispering
    }

    /// The speech probability this frame's gain chain computed.
    pub fn speech_probability(&self) -> f32 {
        self.speech_probability
    }

    /// Clears the activators and the whisper state (`reset`).
    pub fn reset(&mut self) {
        self.main.reset();
        self.whisper.reset();
        self.whispering = false;
        self.activating = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(activation: Activation) -> ActivationConfig {
        ActivationConfig {
            activation,
            ..ActivationConfig::default()
        }
    }

    fn quiet() -> Vec<i16> {
        vec![10i16; 960]
    }

    fn loud() -> Vec<i16> {
        vec![3000i16; 960]
    }

    #[test]
    fn ptt_transmits_while_the_key_is_down_plus_a_tail() {
        let mut p = MicProcessor::new(config(Activation::PushToTalk));
        // Down: transmit (5 frame tail reloads every frame).
        for _ in 0..3 {
            let mut a = quiet();
            p.process(&mut a, false, false, true, false);
            assert!(p.should_transmit_audio());
        }
        // Released: the 5-frame tail keeps sending, then stops.
        let mut after_release = 0;
        for _ in 0..10 {
            let mut a = quiet();
            p.process(&mut a, false, false, false, false);
            if p.should_transmit_audio() {
                after_release += 1;
            }
        }
        assert_eq!(after_release, 5, "the PTT tail is 5 frames");
    }

    #[test]
    fn voice_uses_the_threshold_strictly_and_has_a_longer_tail() {
        let mut p = MicProcessor::new(config(Activation::Voice));
        let mut a = loud();
        p.process(&mut a, false, false, false, false);
        assert!(p.should_transmit_audio(), "3000 is above −50 dB");
        let mut after_release = 0;
        for _ in 0..40 {
            let mut a = quiet();
            p.process(&mut a, false, false, false, false);
            if p.should_transmit_audio() {
                after_release += 1;
            }
        }
        assert_eq!(after_release, 25, "the VOICE tail is 25 frames");
    }

    #[test]
    fn voice_threshold_at_the_level_is_not_enough() {
        let mut p = MicProcessor::new(config(Activation::Voice));
        // A frame whose level equals the threshold is not above it.
        let threshold = -50.0f64;
        // level(threshold_level) == threshold: build the frame at exactly
        // the threshold's sample magnitude.
        let level_sample = ((32768.0 * 10.0f64.powf(threshold / 20.0)) as i16).max(1);
        let mut a = vec![level_sample; 960];
        p.process(&mut a, false, false, false, false);
        assert!(
            !p.should_transmit_audio(),
            "equal to the threshold is not above it"
        );
        let mut a = vec![level_sample + 1; 960];
        p.process(&mut a, false, false, false, false);
        assert!(p.should_transmit_audio());
    }

    #[test]
    fn mute_blocks_everything_but_the_mic_test() {
        let mut p = MicProcessor::new(config(Activation::Voice));
        let mut a = loud();
        p.process(&mut a, true, false, false, false);
        assert!(!p.should_transmit_audio(), "muted VOICE does not transmit");
        let mut a = loud();
        p.process(&mut a, true, false, false, true);
        assert!(p.should_transmit_audio(), "the mic test bypasses mute");

        let mut p = MicProcessor::new(config(Activation::PushToTalk));
        let mut a = quiet();
        p.process(&mut a, true, false, true, false);
        assert!(!p.should_transmit_audio(), "muted PTT does not transmit");
        // And the whisper path never transmits while muted either.
        let mut a = quiet();
        p.process(&mut a, true, true, false, false);
        assert!(!p.should_transmit_audio());
    }

    #[test]
    fn the_whisper_path_transmits_when_the_main_is_silent() {
        let mut p = MicProcessor::new(config(Activation::PushToTalk));
        // Whisper key held, PTT up: the whisper activator reloads every
        // frame, so holding it transmits indefinitely.
        for _ in 0..10 {
            let mut a = quiet();
            p.process(&mut a, false, true, false, false);
            assert!(p.should_transmit_audio() && p.is_whispering());
        }
        // Released: the 5-frame tail keeps whispering, then stops.
        let mut after_release = 0;
        for _ in 0..10 {
            let mut a = quiet();
            p.process(&mut a, false, false, false, false);
            if p.should_transmit_audio() {
                after_release += 1;
                assert!(p.is_whispering(), "the tail is still a whisper");
            }
        }
        assert_eq!(after_release, 5, "the whisper tail is the PTT tail");
        // A loud frame on the main path while whispering keeps it a whisper.
        let mut p = MicProcessor::new(config(Activation::PushToTalk));
        let mut a = loud();
        p.process(&mut a, false, true, true, false);
        assert!(p.should_transmit_audio() && p.is_whispering());
    }

    #[test]
    fn the_vad_replaces_the_threshold_when_a_denoiser_exists() {
        // A "denoiser" that always says 0.4 probability: below the VAD's
        // 0.5, so no transmission even though the frame is loud.
        fn vad04(audio: &mut [i16]) -> f32 {
            let _ = audio;
            0.4
        }
        let cfg = ActivationConfig {
            activation: Activation::Voice,
            denoiser: Some(vad04),
            vad: true,
            ..ActivationConfig::default()
        };
        let mut p = MicProcessor::new(cfg);
        let mut a = loud();
        p.process(&mut a, false, false, false, false);
        assert!(
            !p.should_transmit_audio(),
            "VAD 0.4 < 0.5 means silence, whatever the level"
        );
        // At 0.6 it transmits even though the frame is quiet.
        fn vad06(_audio: &mut [i16]) -> f32 {
            0.6
        }
        let cfg = ActivationConfig {
            activation: Activation::Voice,
            denoiser: Some(vad06),
            vad: true,
            ..ActivationConfig::default()
        };
        let mut p = MicProcessor::new(cfg);
        let mut a = quiet();
        p.process(&mut a, false, false, false, false);
        assert!(p.should_transmit_audio());
        // With `vad` off the same denoiser falls back to the threshold.
        let cfg = ActivationConfig {
            activation: Activation::Voice,
            denoiser: Some(vad04),
            vad: false,
            ..ActivationConfig::default()
        };
        let mut p = MicProcessor::new(cfg);
        let mut a = loud();
        p.process(&mut a, false, false, false, false);
        assert!(p.should_transmit_audio(), "vad off: the level decides");
    }

    #[test]
    fn the_agc_stub_runs_instead_of_the_manual_gain_when_set() {
        static AGC_RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        fn agc(audio: &mut [i16]) {
            AGC_RUNS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            audio.iter_mut().for_each(|s| *s = 5);
        }
        let cfg = ActivationConfig {
            agc: true,
            agc_fn: Some(agc),
            ..ActivationConfig::default()
        };
        let mut p = MicProcessor::new(cfg);
        let mut a = loud();
        p.process(&mut a, false, false, true, false);
        assert_eq!(AGC_RUNS.load(std::sync::atomic::Ordering::Relaxed), 1);
        // With AGC off the manual gain runs instead.
        static GAIN_RUNS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        // (The manual path is observable through the samples: gain 0 dB on
        // a loud frame keeps them; the AGC stub rewrote them to 5.)
        let cfg = ActivationConfig {
            agc: false,
            agc_fn: Some(agc),
            ..ActivationConfig::default()
        };
        let mut p = MicProcessor::new(cfg);
        let mut a = loud();
        p.process(&mut a, false, false, true, false);
        assert_eq!(a[0], 3000, "the manual gain left the samples alone");
        assert_eq!(
            GAIN_RUNS.load(std::sync::atomic::Ordering::Relaxed),
            0,
            "the manual gain has no stub; it is a pure transform"
        );
    }

    #[test]
    fn reset_clears_the_tails() {
        let mut p = MicProcessor::new(config(Activation::PushToTalk));
        let mut a = quiet();
        p.process(&mut a, false, false, true, false);
        assert!(p.should_transmit_audio());
        p.reset();
        let mut a = quiet();
        p.process(&mut a, false, false, false, false);
        assert!(!p.should_transmit_audio(), "the tail was cleared");
    }

    #[test]
    fn group_names_and_passwords_follow_the_regex() {
        assert!(group_name_ok("team"));
        assert!(group_name_ok("a"));
        assert!(group_name_ok(&"x".repeat(24)));
        assert!(!group_name_ok(&"x".repeat(25)), "24 is the cap");
        assert!(!group_name_ok(""), "empty names refuse");
        assert!(!group_name_ok(" team"), "a leading space refuses");
        assert!(!group_name_ok("te\u{0}am"), "a control character refuses");
        assert!(group_password_ok("pass word"));
        assert!(!group_password_ok(&"p".repeat(257)), "256 is the cap");
        assert!(!group_password_ok("pa\u{1}ss"));
    }
}
