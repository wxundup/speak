
// --- the server config (voicechat-server.properties) ------------------------

pub const SERVER_PORT: i32 = 24_454;
/// `max_voice_distance`, default 48 blocks, range [1, 1 000 000].
pub const SERVER_MAX_VOICE_DISTANCE: f64 = 48.0;
/// `whisper_distance`, default 24 blocks, range [1, 1 000 000].
pub const SERVER_WHISPER_DISTANCE: f64 = 24.0;
/// `codec`, default VOIP (`VOIP | AUDIO | RESTRICTED_LOWDELAY`). Our codec
/// is fixed; the option is kept so a config parses.
pub const SERVER_CODEC: &str = "VOIP";
/// `mtu_size`, default 1275 (`AudioUtils.MAX_OPUS_PAYLOAD_SIZE`), range
/// [512, 2048]. Unused on mod_net (its cap is 30 000); kept.
pub const SERVER_MTU_SIZE: i32 = 1275;
/// `tcp_rate_limit`, default 16 packets a second, range [-1, 1024]; -1
/// disables. mod_net has its own per-tick caps; kept.
pub const SERVER_TCP_RATE_LIMIT: i32 = 16;
/// `keep_alive`, default 1000 ms, minimum 1000.
pub const SERVER_KEEP_ALIVE: u64 = 1_000;
/// `enable_groups`, default true.
pub const SERVER_GROUPS_ENABLED: bool = true;
/// `allow_recording`, default true.
pub const SERVER_ALLOW_RECORDING: bool = true;
/// `spectator_interaction`, default false.
pub const SERVER_SPECTATOR_INTERACTION: bool = false;
/// `spectator_player_possession`, default false.
pub const SERVER_SPECTATOR_PLAYER_POSSESSION: bool = false;
/// `force_voice_chat`, default false (Horizon can't kick vanilla clients
/// from a native mod; the option is kept, documented as inert).
pub const SERVER_FORCE_VOICE_CHAT: bool = false;
/// `login_timeout`, default 10 000 ms.
pub const SERVER_LOGIN_TIMEOUT: u64 = 10_000;
/// `broadcast_range`, default −1 (`max_voice_distance + 1`).
pub const SERVER_BROADCAST_RANGE: f64 = -1.0;
/// `allow_pings`, default true (external pings; mod_net pings are ours).
pub const SERVER_ALLOW_PINGS: bool = true;

// --- the client config (voicechat-client.properties) ------------------------

/// `voice_chat_volume`, default 1.0, range [0, 3].
pub const VOICE_CHAT_VOLUME: f64 = 1.0;
/// `voice_activation_threshold`, default −50 dB, range [−127, 0].
pub const VOICE_ACTIVATION_THRESHOLD: f64 = -50.0;
/// `vad` (`voice_activity_detection`), default true.
pub const VOICE_ACTIVITY_DETECTION: bool = true;
/// `microphone_gain`, default 0 dB, range [−40, +24].
pub const MICROPHONE_GAIN: f64 = 0.0;
/// `agc` (`automatic_gain_control`), default true.
pub const AUTOMATIC_GAIN_CONTROL: bool = true;
/// `microphone_activation_type`, default PTT (`PTT | VOICE`).
pub const MICROPHONE_ACTIVATION_TYPE: &str = "PTT";
/// `output_buffer_size`, default 5 packets (100 ms), range [1, 16].
pub const OUTPUT_BUFFER_SIZE: u32 = 5;
/// `audio_packet_threshold`, default 3 packets, range [0, 16]; 0 disables
/// reordering.
pub const AUDIO_PACKET_THRESHOLD: u32 = 3;
/// `voice_deactivation_delay`, default 25 frames (500 ms), range [0, 100].
pub const VOICE_DEACTIVATION_DELAY: u32 = 25;
/// `ptt_deactivation_delay`, default 5 frames (100 ms), range [0, 100].
pub const PTT_DEACTIVATION_DELAY: u32 = 5;
pub const MUTED: bool = true;
/// `disabled`, default false.
pub const DISABLED: bool = false;
/// `hide_icons`, default false.
pub const HIDE_ICONS: bool = false;
/// `show_hud_icons`, default true.
pub const SHOW_HUD_ICONS: bool = true;
/// `show_group_hud`, default true.
pub const SHOW_GROUP_HUD: bool = true;
/// `show_own_group_icon`, default true.
pub const SHOW_OWN_GROUP_ICON: bool = true;
/// `group_hud_icon_scale`, default 2.0, range [0.01, 10].
pub const GROUP_HUD_ICON_SCALE: f64 = 2.0;
/// `group_player_icon_orientation`, default VERTICAL.
pub const GROUP_PLAYER_ICON_ORIENTATION: &str = "VERTICAL";
/// `group_player_icon_pos_x`, default 4 (negative counts from the right).
pub const GROUP_PLAYER_ICON_POS_X: i32 = 4;
/// `group_player_icon_pos_y`, default 4 (negative counts from the bottom).
pub const GROUP_PLAYER_ICON_POS_Y: i32 = 4;
/// `hud_icon_pos_x`, default 16.
pub const HUD_ICON_POS_X: i32 = 16;
/// `hud_icon_pos_y`, default −16 (bottom right).
pub const HUD_ICON_POS_Y: i32 = -16;
/// `hud_icon_scale`, default 1.0, range [0.01, 10].
pub const HUD_ICON_SCALE: f64 = 1.0;
/// `recording_destination`, default "" (the game folder's
/// `voicechat_recordings`).
pub const RECORDING_DESTINATION: &str = "";
pub const RECORDING_QUALITY: i32 = 2;
/// `denoiser`, default true.
pub const DENOISER: bool = true;
pub const RUN_LOCAL_SERVER: bool = true;
/// `audio_type`, default NORMAL (`NORMAL | REDUCED | OFF`): the game's own
/// OpenAL math, software panning, or mono with software distance.
pub const AUDIO_TYPE: &str = "NORMAL";
pub const USE_NATIVES: bool = true;
/// `mute_on_join`, default false.
pub const MUTE_ON_JOIN: bool = false;
/// `config_version`, default 2 (`ClientConfig.CONFIG_VERSION`).
pub const CONFIG_VERSION: i32 = 2;

// --- the ranges, as the settings page clamps to them ------------------------

/// `voice_chat_volume`'s range.
pub const VOICE_CHAT_VOLUME_RANGE: (f64, f64) = (0.0, 3.0);
/// `voice_activation_threshold`'s range.
pub const VOICE_ACTIVATION_THRESHOLD_RANGE: (f64, f64) = (-127.0, 0.0);
/// `microphone_gain`'s range (`VolumeManager.MIN_GAIN`/`MAX_GAIN`).
pub const MICROPHONE_GAIN_RANGE: (f64, f64) = (-40.0, 24.0);
/// `output_buffer_size`'s range.
pub const OUTPUT_BUFFER_SIZE_RANGE: (u32, u32) = (1, 16);
/// `audio_packet_threshold`'s range.
pub const AUDIO_PACKET_THRESHOLD_RANGE: (u32, u32) = (0, 16);
/// The two deactivation delays' range.
pub const DEACTIVATION_DELAY_RANGE: (u32, u32) = (0, 100);
/// `group_hud_icon_scale`'s range.
pub const GROUP_HUD_ICON_SCALE_RANGE: (f64, f64) = (0.01, 10.0);
/// `hud_icon_scale`'s range.
pub const HUD_ICON_SCALE_RANGE: (f64, f64) = (0.01, 10.0);
/// `recording_quality`'s range.
pub const RECORDING_QUALITY_RANGE: (i32, i32) = (0, 9);

/// The mic amplification where the settings page warns in red
/// (`MicAmplificationSlider.GAIN_WARNING_THRESHOLD`).
pub const GAIN_WARNING_THRESHOLD: f64 = 10.0;

/// A per-player volume's clamp (`PlayerVolumeConfig`'s values are
/// unbounded doubles; the sliders clamp to [0, 4] — `AdjustVolumeSlider
/// .MAXIMUM = 4F` — and `%.3f` format).
pub const PLAYER_VOLUME_MAXIMUM: f64 = 4.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_server_defaults_match_simple_voice_chat() {
        assert_eq!(SERVER_PORT, 24_454);
        assert_eq!(SERVER_MAX_VOICE_DISTANCE, 48.0);
        assert_eq!(SERVER_WHISPER_DISTANCE, 24.0);
        assert_eq!(SERVER_CODEC, "VOIP");
        assert_eq!(SERVER_MTU_SIZE, 1275);
        assert_eq!(SERVER_TCP_RATE_LIMIT, 16);
        assert_eq!(SERVER_KEEP_ALIVE, 1_000);
        assert!(SERVER_GROUPS_ENABLED);
        assert!(SERVER_ALLOW_RECORDING);
        assert!(!SERVER_SPECTATOR_INTERACTION);
        assert!(!SERVER_SPECTATOR_PLAYER_POSSESSION);
        assert!(!SERVER_FORCE_VOICE_CHAT);
        assert_eq!(SERVER_LOGIN_TIMEOUT, 10_000);
        assert_eq!(SERVER_BROADCAST_RANGE, -1.0);
        assert!(SERVER_ALLOW_PINGS);
    }

    #[test]
    fn the_client_defaults_match_simple_voice_chat() {
        assert_eq!(CONFIG_VERSION, 2);
        assert_eq!(VOICE_CHAT_VOLUME, 1.0);
        assert_eq!(VOICE_ACTIVATION_THRESHOLD, -50.0);
        assert!(VOICE_ACTIVITY_DETECTION);
        assert_eq!(MICROPHONE_GAIN, 0.0);
        assert!(AUTOMATIC_GAIN_CONTROL);
        assert_eq!(MICROPHONE_ACTIVATION_TYPE, "PTT");
        assert_eq!(OUTPUT_BUFFER_SIZE, 5);
        assert_eq!(AUDIO_PACKET_THRESHOLD, 3);
        assert_eq!(VOICE_DEACTIVATION_DELAY, 25);
        assert_eq!(PTT_DEACTIVATION_DELAY, 5);
        assert!(MUTED, "the first run starts muted (VA-aware)");
        assert!(!DISABLED);
        assert!(!HIDE_ICONS);
        assert!(SHOW_HUD_ICONS);
        assert!(SHOW_GROUP_HUD);
        assert!(SHOW_OWN_GROUP_ICON);
        assert_eq!(GROUP_HUD_ICON_SCALE, 2.0);
        assert_eq!(GROUP_PLAYER_ICON_ORIENTATION, "VERTICAL");
        assert_eq!(GROUP_PLAYER_ICON_POS_X, 4);
        assert_eq!(GROUP_PLAYER_ICON_POS_Y, 4);
        assert_eq!(HUD_ICON_POS_X, 16);
        assert_eq!(HUD_ICON_POS_Y, -16);
        assert_eq!(HUD_ICON_SCALE, 1.0);
        assert_eq!(RECORDING_QUALITY, 2);
        assert!(DENOISER);
        assert!(RUN_LOCAL_SERVER);
        assert_eq!(AUDIO_TYPE, "NORMAL");
        assert!(USE_NATIVES);
        assert!(!MUTE_ON_JOIN);
    }

    #[test]
    fn the_ranges_match_the_sliders() {
        assert_eq!(VOICE_CHAT_VOLUME_RANGE, (0.0, 3.0));
        assert_eq!(VOICE_ACTIVATION_THRESHOLD_RANGE, (-127.0, 0.0));
        assert_eq!(MICROPHONE_GAIN_RANGE, (-40.0, 24.0));
        assert_eq!(OUTPUT_BUFFER_SIZE_RANGE, (1, 16));
        assert_eq!(AUDIO_PACKET_THRESHOLD_RANGE, (0, 16));
        assert_eq!(DEACTIVATION_DELAY_RANGE, (0, 100));
        assert_eq!(GROUP_HUD_ICON_SCALE_RANGE, (0.01, 10.0));
        assert_eq!(HUD_ICON_SCALE_RANGE, (0.01, 10.0));
        assert_eq!(RECORDING_QUALITY_RANGE, (0, 9));
    }

    #[test]
    fn the_warning_threshold_and_the_volume_cap_match() {
        assert_eq!(GAIN_WARNING_THRESHOLD, 10.0);
        assert_eq!(PLAYER_VOLUME_MAXIMUM, 4.0);
    }
}
