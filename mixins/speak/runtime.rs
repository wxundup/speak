// Speak. — a proximity voice chat native mod for Horizon.
//
// Hold a key (V by default) to talk and the players near you hear you, or join a
// group to talk across distance. Whispering shortens the distance. Talking players
// are shown above their heads, and each listener can set a per-player volume.
//
// This file is both the mod's Cargo library (its tests) and the module the bundle
// adds to the game as `game::official_speak`. Everything here is PURE Rust: no
// references to `crate::game`, so it compiles on its own with `cargo test`. The
// helpers in mixins/speak/*.rs wrap these functions with `game`'s helper methods.
//
// Design notes, ported from Simple Voice Chat:
//   - Mic level is read each tick and turned into Opus audio frames.
//   - The client sends MicPacket over the mod channel; the server mixes it by
//     proximity (in_range) or by group (groups) and sends SoundPacket back.
//   - Settings and key bindings are stored one file per key under the mod's
//     settings folder, read through `native_setting`.

use std::cell::RefCell;
use std::collections::HashMap;

/// The mod's id: its settings folder, its key's line and its mod channel.
pub const ID: &str = "speak";
pub const VERSION: &str = "0.1.0";
pub const DESCRIPTION: &str =
    "Hold a key to talk; nearby players hear you, groups reach further. Horizon's own.";
/// The key's name, and its default.
pub const KEY: &str = "speak";
pub const DEFAULT_KEY: &str = "key.keyboard.v";

/// The default voice-chat distance, in blocks, and the whisper distance.
pub const DEFAULT_DISTANCE: f32 = 100.0;
pub const DEFAULT_WHISPER_DISTANCE: f32 = 12.0;
/// Hard limits the settings page clamps to.
pub const MIN_DISTANCE: f32 = 8.0;
pub const MAX_DISTANCE: f32 = 1024.0;

/// The maximum size of one Opus audio frame the mod sends, in bytes.
pub const MAX_FRAME_BYTES: usize = 4096;
/// How many ticks a talking indicator stays visible after the last frame.
pub const TALKING_FADE_TICKS: u64 = 10;

/// The mod channel the voice packets travel on (`speak:voice`).
pub const CHANNEL: &str = "speak:voice";

/// How a player transmits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// Transmit only while the key is held.
    #[default]
    PushToTalk,
    /// Transmit whenever the mic level passes the sensitivity threshold.
    VoiceActivation,
}

/// The transmission side of a sound packet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    /// Ordinary proximity speech.
    Proximity,
    /// A whisper (short distance).
    Whisper,
    /// A group chat.
    Group,
}

/// The settings, stored one value per key under the mod's settings folder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    /// Push to talk or voice activation.
    pub mode: Mode,
    /// How far a normal voice carries, in blocks.
    pub distance: f32,
    /// How far a whisper carries, in blocks.
    pub whisper_distance: f32,
    /// Mic level above which voice activation transmits (0.0..=1.0).
    pub mic_sensitivity: f32,
    /// Output volume for incoming voices (0.0..=2.0).
    pub volume: f32,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            mode: Mode::PushToTalk,
            distance: DEFAULT_DISTANCE,
            whisper_distance: DEFAULT_WHISPER_DISTANCE,
            mic_sensitivity: 0.5,
            volume: 1.0,
        }
    }
}

impl Settings {
    /// From the stored values (`get` reads one key's file); the defaults for the
    /// ones missing.
    pub fn read(get: impl Fn(&str) -> Option<String>) -> Self {
        let number = |key: &str, default: f32| {
            get(key)
                .and_then(|v| v.trim().parse::<f32>().ok())
                .unwrap_or(default)
        };
        Self {
            mode: if get("mode").as_deref() == Some("vox") {
                Mode::VoiceActivation
            } else {
                Mode::PushToTalk
            },
            distance: number("distance", DEFAULT_DISTANCE)
                .clamp(MIN_DISTANCE, MAX_DISTANCE),
            whisper_distance: number("whisper_distance", DEFAULT_WHISPER_DISTANCE)
                .clamp(MIN_DISTANCE, MAX_DISTANCE),
            mic_sensitivity: number("mic_sensitivity", 0.5).clamp(0.0, 1.0),
            volume: number("volume", 1.0).clamp(0.0, 2.0),
        }
    }

    /// The settings page's buttons, as labels the player can click.
    pub fn labels(&self) -> Vec<String> {
        vec![
            format!(
                "Mode: {}",
                match self.mode {
                    Mode::PushToTalk => "push to talk",
                    Mode::VoiceActivation => "voice activation",
                }
            ),
            format!("Distance: {:.0} blocks", self.distance),
            format!("Whisper: {:.0} blocks", self.whisper_distance),
            format!("Mic sensitivity: {:.0}", self.mic_sensitivity * 100.0),
            format!("Volume: {:.0}", self.volume * 100.0),
        ]
    }

    /// A click on button `i` of `labels`.
    pub fn click(&mut self, i: usize) {
        match i {
            0 => self.mode = match self.mode {
                Mode::PushToTalk => Mode::VoiceActivation,
                Mode::VoiceActivation => Mode::PushToTalk,
            },
            1 => self.distance = (self.distance + 50.0).clamp(MIN_DISTANCE, MAX_DISTANCE),
            2 => self.whisper_distance = (self.whisper_distance + 5.0)
                .clamp(MIN_DISTANCE, MAX_DISTANCE),
            3 => self.mic_sensitivity = (self.mic_sensitivity + 0.2).clamp(0.0, 1.0),
            4 => self.volume = (self.volume + 0.25).clamp(0.0, 2.0),
            _ => {}
        }
    }

    /// What to store, key by key.
    pub fn stored(&self) -> [(&'static str, String); 5] {
        [
            ("mode", match self.mode {
                Mode::PushToTalk => "pt".to_owned(),
                Mode::VoiceActivation => "vox".to_owned(),
            }),
            ("distance", self.distance.to_string()),
            ("whisper_distance", self.whisper_distance.to_string()),
            ("mic_sensitivity", self.mic_sensitivity.to_string()),
            ("volume", self.volume.to_string()),
        ]
    }
}

/// One player's current voice state, as the server tracks it.
#[derive(Clone, Debug)]
pub struct PlayerVoice {
    pub uuid: u128,
    pub name: String,
    pub dimension: String,
    pub position: [f64; 3],
    /// Whether this player is currently transmitting.
    pub talking: bool,
    /// The last mic level, 0.0..=1.0.
    pub voice_level: f32,
    /// The tick the last frame was received, for the talking indicator.
    pub last_heard: u64,
    /// The group this player is in, if any.
    pub group: Option<u128>,
}

/// One named, password-protected group.
#[derive(Clone, Debug)]
pub struct Group {
    pub id: u128,
    pub name: String,
    /// Whether anyone who knows the name can join, or a password is required.
    pub open: bool,
    /// The members of the group.
    pub members: HashMap<u128, String>,
}

/// A voice packet from a client, as decoded.
#[derive(Clone, Debug)]
pub struct MicPacket {
    pub data: Vec<u8>,
    pub sequence: u32,
    pub whisper: bool,
    pub group: Option<u128>,
}

/// A sound packet sent to a listener, as decoded.
#[derive(Clone, Debug)]
pub struct SoundPacket {
    pub sender: u128,
    pub sender_name: String,
    pub dimension: String,
    pub position: [f64; 3],
    pub voice_level: f32,
}

/// The mod's runtime state.
#[derive(Default)]
struct State {
    /// Read on the first key press, then kept.
    settings: Option<Settings>,
    /// Whether the local player is transmitting right now.
    talking: bool,
    /// The local player's last mic level.
    voice_level: f32,
    /// The players the local client knows about, keyed by uuid.
    players: HashMap<u128, PlayerVoice>,
    /// The groups the local player belongs to.
    groups: HashMap<u128, Group>,
    /// The tick the local player started talking, for the fade.
    talking_since: u64,
    /// Voice packets received from clients this tick, queued for the server mix.
    queued_packets: Vec<(u128, Vec<u8>)>,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The settings, read once through `get`.
pub fn settings(get: impl Fn(&str) -> Option<String>) -> Settings {
    STATE.with_borrow_mut(|s| *s.settings.get_or_insert_with(|| Settings::read(get)))
}

/// Replaces the settings (the settings page changed them).
pub fn set_settings(settings: Settings) {
    STATE.with_borrow_mut(|s| s.settings = Some(settings));
}

/// The key went down or up: start or stop transmitting.
pub fn key(playing: bool, pressed: bool, get: impl Fn(&str) -> Option<String>) {
    if pressed && !playing {
        return;
    }
    let _ = settings(get); // ensure settings exist so the key is bound
    STATE.with_borrow_mut(|s| {
        s.talking = pressed;
        if pressed {
            s.talking_since = 0; // filled in by the first tick
        }
    });
}

/// The mouse wheel nudges the local output volume while the key is held.
pub fn wheel(steps: f32) -> bool {
    STATE.with_borrow_mut(|s| {
        let Some(mut settings) = s.settings else {
            return false;
        };
        settings.volume = (settings.volume + steps * 0.1).clamp(0.0, 2.0);
        s.settings = Some(settings);
        true
    })
}

/// The mic level this tick (0.0..=1.0), from the audio capture.
pub fn mic_level() -> f32 {
    STATE.with_borrow(|s| s.voice_level)
}

/// Sets the mic level this tick (called from the player_tick helper).
pub fn set_mic_level(level: f32) {
    STATE.with_borrow_mut(|s| {
        s.voice_level = level.clamp(0.0, 1.0);
    });
}

/// Whether the local player is transmitting right now.
pub fn talking() -> bool {
    STATE.with_borrow(|s| s.talking)
}

/// The voice level the overlay shows for the local player.
pub fn local_voice_level() -> f32 {
    STATE.with_borrow(|s| s.voice_level)
}

/// The players this client knows about, for the talking overlay.
pub fn players() -> HashMap<u128, PlayerVoice> {
    STATE.with_borrow(|s| s.players.clone())
}

/// Records a voice packet received from the server: who spoke, where, and how loud.
pub fn heard(
    uuid: u128,
    name: String,
    dimension: String,
    position: [f64; 3],
    voice_level: f32,
    tick: u64,
) {
    STATE.with_borrow_mut(|s| {
        s.players.insert(
            uuid,
            PlayerVoice {
                uuid,
                name,
                dimension,
                position,
                talking: true,
                voice_level,
                last_heard: tick,
                group: None,
            },
        );
    });
}

/// Clears talking indicators whose last frame is older than the fade.
pub fn tick_players(current_tick: u64) {
    STATE.with_borrow_mut(|s| {
        s.players
            .retain(|_, v| current_tick - v.last_heard <= TALKING_FADE_TICKS);
    });
}

/// Whether two positions are within `distance` blocks of each other (flat distance).
pub fn in_range(a: [f64; 3], b: [f64; 3], distance: f32) -> bool {
    let dx = a[0] - b[0];
    let dy = a[1] - b[1];
    let dz = a[2] - b[2];
    (dx * dx + dy * dy + dz * dz).sqrt() <= f64::from(distance)
}

/// Called when a world opens: reset the voice state for the new world.
pub fn opened() {
    STATE.with_borrow_mut(|s| {
        s.players.clear();
        s.groups.clear();
        s.queued_packets.clear();
    });
}

/// Called when a world closes: drop the voice state.
pub fn closed() {
    STATE.with_borrow_mut(|s| {
        s.players.clear();
        s.groups.clear();
        s.queued_packets.clear();
        // talking is client-side; reset it anyway for cleanliness.
        s.talking = false;
    });
}

/// Queue a voice packet received from a client this tick, for the server mix.
pub fn queue_packet(uuid: u128, data: Vec<u8>) {
    STATE.with_borrow_mut(|s| {
        s.queued_packets.push((uuid, data));
    });
}

/// Drain and return the packets queued this tick.
pub fn drain_packets() -> Vec<(u128, Vec<u8>)> {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.queued_packets))
}

/// The broadcast range, clamped so nearby players always hear.
pub fn broadcast_range(distance: f32) -> f32 {
    distance.clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// Whether the talking-player overlay is enabled.
pub fn overlay_enabled() -> bool {
    STATE.with_borrow(|s| s.settings.is_some())
}

/// Encodes a mic packet from a level (stub: real Opus framing goes here).
pub fn encode_mic_packet(_level: f32) -> Vec<u8> {
    Vec::new()
}

/// Decodes a mic packet (stub: real Opus framing goes here).
pub fn decode_mic_packet(_data: &[u8]) -> Option<MicPacket> {
    None
}

/// Encodes a sound packet for a listener (stub: real Opus framing goes here).
pub fn encode_sound_packet(
    _sender: u128,
    _sender_name: String,
    _data: Vec<u8>,
    _sequence: u32,
    _whisper: bool,
    _range: f32,
) -> Vec<u8> {
    Vec::new()
}

/// Decodes a sound packet (stub: real Opus framing goes here).
pub fn decode_sound_packet(_data: &[u8]) -> Option<SoundPacket> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stored<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
        |key| pairs.iter().find(|p| p.0 == key).map(|p| p.1.to_owned())
    }

    #[test]
    fn reads_the_default_settings() {
        let s = Settings::read(stored(&[]));
        assert_eq!(s.mode, Mode::PushToTalk);
        assert_eq!(s.distance, DEFAULT_DISTANCE);
        assert_eq!(s.whisper_distance, DEFAULT_WHISPER_DISTANCE);
        assert_eq!(s.mic_sensitivity, 0.5);
        assert_eq!(s.volume, 1.0);
    }

    #[test]
    fn reads_stored_settings() {
        let s = Settings::read(stored(&[
            ("mode", "vox"),
            ("distance", "200"),
            ("whisper_distance", "20"),
            ("mic_sensitivity", "0.8"),
            ("volume", "1.5"),
        ]));
        assert_eq!(s.mode, Mode::VoiceActivation);
        assert_eq!(s.distance, 200.0);
        assert_eq!(s.whisper_distance, 20.0);
        assert_eq!(s.mic_sensitivity, 0.8);
        assert_eq!(s.volume, 1.5);
    }

    #[test]
    fn clamps_distances() {
        let s = Settings::read(stored(&[("distance", "9999")]));
        assert_eq!(s.distance, MAX_DISTANCE);
        let s = Settings::read(stored(&[("distance", "1")]));
        assert_eq!(s.distance, MIN_DISTANCE);
    }

    #[test]
    fn clicks_cycle_mode_and_nudge_values() {
        let mut s = Settings::default();
        assert_eq!(s.mode, Mode::PushToTalk);
        s.click(0);
        assert_eq!(s.mode, Mode::VoiceActivation);
        s.click(0);
        assert_eq!(s.mode, Mode::PushToTalk);

        let mut s = Settings::default();
        s.click(1);
        assert!((s.distance - (DEFAULT_DISTANCE + 50.0)).abs() < f32::EPSILON);
        s.click(2);
        assert!((s.whisper_distance - (DEFAULT_WHISPER_DISTANCE + 5.0)).abs()
            < f32::EPSILON);
        s.click(3);
        assert!((s.mic_sensitivity - 0.7).abs() < f32::EPSILON);
        s.click(4);
        assert!((s.volume - 1.25).abs() < f32::EPSILON);
    }

    #[test]
    fn stored_roundtrips() {
        let s = Settings::read(stored(&[
            ("mode", "vox"),
            ("distance", "160"),
            ("whisper_distance", "16"),
            ("mic_sensitivity", "0.75"),
            ("volume", "1.25"),
        ]));
        let round: Vec<(&str, String)> = s.stored().into();
        let back: Vec<(&str, &str)> = round.iter().map(|(k, v)| (*k, v.as_str())).collect();
        let again = Settings::read(stored(&back));
        assert_eq!(again.mode, Mode::VoiceActivation);
        assert!((again.distance - 160.0).abs() < f32::EPSILON);
    }

    #[test]
    fn in_range_measures_flat_distance() {
        assert!(in_range([0.0, 0.0, 0.0], [3.0, 4.0, 0.0], 10.0)); // 5 blocks
        assert!(!in_range([0.0, 0.0, 0.0], [100.0, 0.0, 0.0], 10.0));
    }

    #[test]
    fn talking_and_heard_track_the_overlay() {
        let none = stored(&[]);
        key(true, true, &none);
        assert!(talking());
        key(true, false, &none);
        assert!(!talking());

        heard(
            1,
            "Alex".to_owned(),
            "minecraft:overworld".to_owned(),
            [10.0, 64.0, 10.0],
            0.8,
            5,
        );
        let voices = players();
        assert_eq!(voices.len(), 1);
        let v = voices.get(&1).unwrap();
        assert_eq!(v.name, "Alex");
        assert_eq!(v.voice_level, 0.8);

        // After the fade the indicator disappears.
        tick_players(5 + TALKING_FADE_TICKS + 1);
        assert!(players().is_empty());
    }

    #[test]
    fn queue_and_drain_packets() {
        queue_packet(1, vec![1, 2, 3]);
        queue_packet(2, vec![4, 5]);
        let packets = drain_packets();
        assert_eq!(packets.len(), 2);
        let drained = drain_packets();
        assert!(drained.is_empty());
    }
}
