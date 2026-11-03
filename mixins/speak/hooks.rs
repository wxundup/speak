
/// The mod's id: its settings folder, its key's line and its mod channel.
pub const ID: &str = "speak";
/// The version, for the Mods page.
pub const VERSION: &str = "1.0.0";
/// The description, for the Mods page.
pub const DESCRIPTION: &str =
    "Proximity voice chat: hold a key to talk, groups reach further.";
/// The mod channel the voice and control packets travel on.
pub const CHANNEL: &str = "speak:voice";

pub const KEYS: [(&str, &str, &str); 6] = [
    ("speak", "Speak key", "key.keyboard.caps.lock"),
    ("menu", "Open the voice menu", "key.keyboard.v"),
    ("mute", "Mute microphone", "key.keyboard.m"),
    ("whisper", "Whisper (hold)", "key.keyboard.b"),
    ("hide_icons", "Hide icons", "key.keyboard.h"),
    ("disable", "Disable voice chat", "key.keyboard.n"),
];

pub mod settings {
    /// `voice_chat_volume`, as a string of the value.
    pub const VOICE_VOLUME: &str = "voice_chat_volume";
    /// `microphone_activation_type` ("PTT" | "VOICE").
    pub const ACTIVATION: &str = "microphone_activation_type";
    /// `microphone_gain`, in dB.
    pub const MIC_GAIN: &str = "microphone_gain";
    /// `agc`.
    pub const AGC: &str = "agc";
    /// `denoiser`.
    pub const DENOISER: &str = "denoiser";
    /// `vad`.
    pub const VAD: &str = "vad";
    /// `voice_activation_threshold`, in dB.
    pub const VA_THRESHOLD: &str = "voice_activation_threshold";
    /// `voice_deactivation_delay`, in frames.
    pub const VOICE_DELAY: &str = "voice_deactivation_delay";
    /// `ptt_deactivation_delay`, in frames.
    pub const PTT_DELAY: &str = "ptt_deactivation_delay";
    /// `muted`.
    pub const MUTED: &str = "muted";
    /// `disabled`.
    pub const DISABLED: &str = "disabled";
    /// `hide_icons`.
    pub const HIDE_ICONS: &str = "hide_icons";
    /// `deafened`.
    pub const DEAFENED: &str = "deafened";
    /// `distance` (the server config's max_voice_distance, client-tunable).
    pub const DISTANCE: &str = "distance";
    /// `whisper_distance`.
    pub const WHISPER_DISTANCE: &str = "whisper_distance";
    /// `volume` (the client's own output volume, 0..2).
    pub const VOLUME: &str = "volume";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_simple_voice_chats_defaults() {
        assert_eq!(KEYS[0], ("speak", "Speak key", "key.keyboard.caps.lock"));
        assert_eq!(KEYS[1], ("menu", "Open the voice menu", "key.keyboard.v"));
        assert_eq!(KEYS[2], ("mute", "Mute microphone", "key.keyboard.m"));
        assert_eq!(KEYS[3], ("whisper", "Whisper (hold)", "key.keyboard.b"));
        assert_eq!(KEYS[4], ("hide_icons", "Hide icons", "key.keyboard.h"));
        assert_eq!(KEYS[5], ("disable", "Disable voice chat", "key.keyboard.n"));
    }

    #[test]
    fn the_channel_is_the_mods_own() {
        assert_eq!(CHANNEL, "speak:voice");
        assert_eq!(ID, "speak");
    }

    #[test]
    fn the_settings_names_cover_the_client_config() {
        assert_eq!(settings::VOICE_VOLUME, "voice_chat_volume");
        assert_eq!(settings::ACTIVATION, "microphone_activation_type");
        assert_eq!(settings::MUTED, "muted");
        assert_eq!(settings::DISABLED, "disabled");
        assert_eq!(settings::HIDE_ICONS, "hide_icons");
        assert_eq!(settings::DEAFENED, "deafened");
        assert_eq!(settings::DISTANCE, "distance");
        assert_eq!(settings::WHISPER_DISTANCE, "whisper_distance");
        assert_eq!(settings::VOLUME, "volume");
    }
}
