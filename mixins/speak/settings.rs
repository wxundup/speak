
use super::packets::NativeCommand;

pub enum Button {
    /// `voice_chat_volume` as a percent, cycling 0→25→…→300→0.
    VoiceVolume,
    /// `microphone_activation_type`: PTT ↔ VOICE.
    Activation,
    /// `microphone_gain` in dB, cycling in 6 dB steps from −40 to +24.
    MicGain,
    /// `agc`: ON ↔ OFF.
    Agc,
    /// `denoiser`: ON ↔ OFF.
    Denoiser,
    /// `vad`: ON ↔ OFF (VOICE's decision source).
    Vad,
    /// `voice_activation_threshold` in dB, cycling 5 dB steps.
    VaThreshold,
    /// `voice_deactivation_delay` in frames, cycling 5s.
    VoiceDelay,
    /// `ptt_deactivation_delay` in frames, cycling 5s.
    PttDelay,
    /// `muted`: ON ↔ OFF.
    Muted,
    /// `disabled`: ON ↔ OFF.
    Disabled,
    /// `hide_icons`: ON ↔ OFF.
    HideIcons,
    /// The last heard player's own volume, stepping 25%.
    PlayerVolume,
}

/// Every button, in the page's order.
pub const BUTTONS: [Button; 13] = [
    Button::VoiceVolume,
    Button::Activation,
    Button::MicGain,
    Button::Agc,
    Button::Denoiser,
    Button::Vad,
    Button::VaThreshold,
    Button::VoiceDelay,
    Button::PttDelay,
    Button::Muted,
    Button::Disabled,
    Button::HideIcons,
    Button::PlayerVolume,
];

pub struct Values {
    pub voice_volume: f64,
    pub activation_ptt: bool,
    pub mic_gain: f64,
    pub agc: bool,
    pub denoiser: bool,
    pub vad: bool,
    pub va_threshold: f64,
    pub voice_delay: u32,
    pub ptt_delay: u32,
    pub muted: bool,
    pub disabled: bool,
    pub hide_icons: bool,
    pub player_volume: f32,
    /// The last heard player's name, for the volume row's label.
    pub player_name: Option<String>,
}

pub fn labels(values: &Values) -> Vec<String> {
    let on = |b: bool| if b { "ON" } else { "OFF" };
    let mut out = vec![
        format!("Voice volume: {:.0}", values.voice_volume * 100.0),
        format!(
            "Activation: {}",
            if values.activation_ptt { "push to talk" } else { "voice" }
        ),
        format!("Mic gain: {:+.0} dB", values.mic_gain),
        format!("AGC: {}", on(values.agc)),
        format!("Denoiser: {}", on(values.denoiser)),
        format!("VAD: {}", on(values.vad)),
        format!("VA threshold: {:.0} dB", values.va_threshold),
        format!("Voice tail: {} frames", values.voice_delay),
        format!("PTT tail: {} frames", values.ptt_delay),
        format!("Muted: {}", on(values.muted)),
        format!("Voice chat: {}", on(!values.disabled)),
        format!("Hide icons: {}", on(values.hide_icons)),
    ];
    match (&values.player_name, values.player_volume) {
        (Some(name), volume) => {
            out.push(format!("{}: {:.0}", name, volume * 100.0))
        }
        (None, _) => out.push("Player volume: nobody heard yet".into()),
    }
    out
}

pub fn click(index: usize, values: &Values) -> Option<(&'static str, String)> {
    let step = |current: f64, stride: f64, lo: f64, hi: f64| {
        let next = current + stride;
        if next > hi {
            lo
        } else {
            next
        }
    };
    match BUTTONS.get(index)? {
        Button::VoiceVolume => Some((
            "voice_chat_volume",
            step(values.voice_volume, 0.25, 0.0, 3.0).to_string(),
        )),
        Button::Activation => Some((
            "microphone_activation_type",
            if values.activation_ptt { "VOICE".into() } else { "PTT".into() },
        )),
        Button::MicGain => Some((
            "microphone_gain",
            step(values.mic_gain, 6.0, -40.0, 24.0).to_string(),
        )),
        Button::Agc => Some(("agc", (!values.agc).to_string())),
        Button::Denoiser => Some(("denoiser", (!values.denoiser).to_string())),
        Button::Vad => Some(("vad", (!values.vad).to_string())),
        Button::VaThreshold => Some((
            "voice_activation_threshold",
            step(values.va_threshold, 5.0, -127.0, 0.0).to_string(),
        )),
        Button::VoiceDelay => Some((
            "voice_deactivation_delay",
            (if values.voice_delay >= 100 {
                0
            } else {
                values.voice_delay + 5
            })
            .to_string(),
        )),
        Button::PttDelay => Some((
            "ptt_deactivation_delay",
            (if values.ptt_delay >= 100 {
                0
            } else {
                values.ptt_delay + 5
            })
            .to_string(),
        )),
        Button::Muted => Some(("muted", (!values.muted).to_string())),
        Button::Disabled => Some(("disabled", (!values.disabled).to_string())),
        Button::HideIcons => Some(("hide_icons", (!values.hide_icons).to_string())),
        // The player volume's click steps 25%, wrapping at 200.
        Button::PlayerVolume if values.player_name.is_some() => {
            let mut next = values.player_volume + 0.25;
            if next > 2.0 {
                next = 0.0;
            }
            // The key is `player_<uuid>`; the caller resolves the uuid
            // from the same `last_talker` the label read.
            Some(("__player_volume__", next.to_string()))
        }
        Button::PlayerVolume => None,
    }
}

pub fn commands() -> Vec<NativeCommand> {
    vec![]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        Values {
            voice_volume: 1.0,
            activation_ptt: true,
            mic_gain: 0.0,
            agc: true,
            denoiser: true,
            vad: true,
            va_threshold: -50.0,
            voice_delay: 25,
            ptt_delay: 5,
            muted: false,
            disabled: false,
            hide_icons: false,
            player_volume: 1.0,
            player_name: Some("Alex".into()),
        }
    }

    #[test]
    fn the_labels_cover_every_button_in_order() {
        let labels = labels(&values());
        assert_eq!(labels.len(), BUTTONS.len());
        assert!(labels[0].starts_with("Voice volume: 100"));
        assert!(labels[1].contains("push to talk"));
        assert!(labels[2].contains("+0 dB"));
        assert!(labels[9].contains("Muted: OFF"));
        assert!(labels[11].contains("Hide icons: OFF"));
        assert!(labels[12].starts_with("Alex: 100"));
    }

    #[test]
    fn the_volume_row_names_the_absent_case() {
        let mut values = values();
        values.player_name = None;
        let labels = labels(&values);
        assert_eq!(labels[12], "Player volume: nobody heard yet");
    }

    #[test]
    fn the_volume_cycles_in_quarters_up_to_three() {
        let mut values = values();
        values.voice_volume = 3.0;
        assert_eq!(
            click(0, &values),
            Some(("voice_chat_volume", "0".to_owned())),
            "3.0 wraps to 0"
        );
        values.voice_volume = 1.0;
        assert_eq!(
            click(0, &values),
            Some(("voice_chat_volume", "1.25".to_owned()))
        );
    }

    #[test]
    fn the_toggles_flip() {
        let values = values();
        assert_eq!(click(3, &values), Some(("agc", "false".to_owned())));
        assert_eq!(click(4, &values), Some(("denoiser", "false".to_owned())));
        assert_eq!(click(5, &values), Some(("vad", "false".to_owned())));
        assert_eq!(click(9, &values), Some(("muted", "true".to_owned())));
        assert_eq!(click(10, &values), Some(("disabled", "true".to_owned())));
        assert_eq!(click(11, &values), Some(("hide_icons", "true".to_owned())));
        assert_eq!(
            click(1, &values),
            Some(("microphone_activation_type", "VOICE".to_owned()))
        );
    }

    #[test]
    fn the_gains_step_by_six_and_wrap_at_the_rails() {
        let mut values = values();
        values.mic_gain = 24.0;
        assert_eq!(click(2, &values), Some(("microphone_gain", "-40".to_owned())));
        values.mic_gain = 0.0;
        assert_eq!(click(2, &values), Some(("microphone_gain", "6".to_owned())));
    }

    #[test]
    fn the_va_threshold_steps_by_five() {
        let mut values = values();
        values.va_threshold = -50.0;
        assert_eq!(
            click(6, &values),
            Some(("voice_activation_threshold", "-45".to_owned()))
        );
        values.va_threshold = 0.0;
        assert_eq!(
            click(6, &values),
            Some(("voice_activation_threshold", "-127".to_owned()))
        );
    }

    #[test]
    fn the_tails_step_by_five_and_wrap_at_a_hundred() {
        let mut values = values();
        values.voice_delay = 100;
        assert_eq!(click(7, &values), Some(("voice_deactivation_delay", "0".to_owned())));
        values.ptt_delay = 5;
        assert_eq!(click(8, &values), Some(("ptt_deactivation_delay", "10".to_owned())));
    }

    #[test]
    fn the_player_volume_steps_25_and_wraps_at_200() {
        let mut values = values();
        values.player_volume = 2.0;
        assert_eq!(
            click(12, &values),
            Some(("__player_volume__", "0".to_owned())),
            "200 wraps to 0"
        );
        values.player_volume = 1.0;
        assert_eq!(
            click(12, &values),
            Some(("__player_volume__", "1.25".to_owned()))
        );
        // No player heard: the row's click does nothing.
        values.player_name = None;
        assert_eq!(click(12, &values), None);
    }

    #[test]
    fn an_out_of_range_click_changes_nothing() {
        let values = values();
        assert_eq!(click(13, &values), None);
        assert_eq!(click(999, &values), None);
    }

    #[test]
    fn the_settings_module_registers_no_commands() {
        assert!(commands().is_empty());
    }
}
