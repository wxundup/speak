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
//   - The client captures real PCM frames (16-bit little-endian, mono, 48 kHz,
//     20 ms) while the push-to-talk key is held, and sends MicPacket over the
//     mod channel. Nothing is synthesized: a mic level alone is never audio.
//   - The server mixes by proximity (in_range), from its own player table:
//     what a client said about its position is not trusted. SoundPacket goes
//     back to the listeners near the speaker, in the same dimension.
//   - Settings and key bindings are stored one file per key under the mod's
//     settings folder, read through `native_setting`.
//
// Groups, custom crypto and the talking overlay's per-player volumes are
// deliberately not in this MVP.

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

/// The maximum size of one PCM audio frame the mod sends, in bytes.
pub const MAX_FRAME_BYTES: usize = 1_920;
/// The PCM the mod carries: 16-bit samples, one channel, 48 000 a second.
pub const SAMPLE_RATE: u32 = 48_000;
/// One frame is 20 ms of audio: 960 samples, 1920 bytes.
pub const FRAME_MS: u32 = 20;
pub const FRAME_SAMPLES: usize = 960;
/// The longest a sender name or dimension id may be on the wire.
pub const MAX_STRING_BYTES: usize = 64;
/// How many voice packets a tick may queue for the server mix, and how many
/// sound packets the client keeps for playback, before the oldest are dropped.
pub const MAX_QUEUED_PACKETS: usize = 64;
pub const MAX_PLAYBACK_PACKETS: usize = 64;
/// The wire format's version, first byte of every packet.
pub const WIRE_VERSION: u8 = 1;
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
                .filter(|value| value.is_finite())
                .unwrap_or(default)
        };
        Self {
            mode: if get("mode").as_deref() == Some("vox") {
                Mode::VoiceActivation
            } else {
                Mode::PushToTalk
            },
            distance: number("distance", DEFAULT_DISTANCE).clamp(MIN_DISTANCE, MAX_DISTANCE),
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
            0 => {
                self.mode = match self.mode {
                    Mode::PushToTalk => Mode::VoiceActivation,
                    Mode::VoiceActivation => Mode::PushToTalk,
                }
            }
            1 => self.distance = (self.distance + 50.0).clamp(MIN_DISTANCE, MAX_DISTANCE),
            2 => {
                self.whisper_distance =
                    (self.whisper_distance + 5.0).clamp(MIN_DISTANCE, MAX_DISTANCE)
            }
            3 => self.mic_sensitivity = (self.mic_sensitivity + 0.2).clamp(0.0, 1.0),
            4 => self.volume = (self.volume + 0.25).clamp(0.0, 2.0),
            _ => {}
        }
    }

    /// What to store, key by key.
    pub fn stored(&self) -> [(&'static str, String); 5] {
        [
            (
                "mode",
                match self.mode {
                    Mode::PushToTalk => "pt".to_owned(),
                    Mode::VoiceActivation => "vox".to_owned(),
                },
            ),
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

/// A voice packet from a client, as decoded. `data` is 20 ms of real PCM:
/// 16-bit little-endian mono samples at `SAMPLE_RATE`. `group` is kept for a
/// later release; this version refuses to carry it.
#[derive(Clone, Debug)]
pub struct MicPacket {
    pub data: Vec<u8>,
    pub sequence: u32,
    pub whisper: bool,
    pub group: Option<u128>,
}

/// A sound packet sent to a listener, as decoded. Who spoke (`sender`,
/// `sender_name`), in which `dimension` and at which `position`, both from
/// the server's own player table, and the frame itself: `data` is the
/// sender's PCM and `sequence` the sender's frame number.
#[derive(Clone, Debug)]
pub struct SoundPacket {
    pub sender: u128,
    pub sender_name: String,
    pub dimension: String,
    pub position: [f64; 3],
    pub sequence: u32,
    pub data: Vec<u8>,
}

/// The mod's runtime state. One per thread: the window's game and the
/// server's game each keep their own, so the client's queues and the
/// server's mix never see each other.
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
    /// The groups the local player belongs to (unused in this MVP).
    groups: HashMap<u128, Group>,
    /// Voice packets received from clients this tick, queued for the server mix.
    queued_packets: Vec<(u128, Vec<u8>)>,
    /// Sound packets received from the server, queued for playback.
    playback: Vec<SoundPacket>,
    /// The local player's next outgoing frame number.
    sequence: u32,
    /// Whether the capture device is enabled right now (the client's).
    capture_on: bool,
    /// Non-paused ticks since the world opened (server: ticks, client: frames).
    tick: u64,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State::default());
}

/// The settings, read once through `get`.
pub fn settings(get: impl Fn(&str) -> Option<String>) -> Settings {
    STATE.with_borrow_mut(|s| *s.settings.get_or_insert_with(|| Settings::read(get)))
}

/// Reload persisted settings, including changes made by another game thread.
pub fn refresh_settings(get: impl Fn(&str) -> Option<String>) -> Settings {
    let value = Settings::read(get);
    set_settings(value);
    value
}

/// Replaces the settings (the settings page changed them).
pub fn set_settings(settings: Settings) {
    STATE.with_borrow_mut(|s| s.settings = Some(settings));
}

/// The key went down or up: start or stop transmitting. A new hold starts
/// the frame numbers over, so the server reads a fresh stream.
pub fn key(playing: bool, pressed: bool, get: impl Fn(&str) -> Option<String>) {
    if pressed && !playing {
        return;
    }
    let _ = settings(get); // ensure settings exist so the key is bound
    STATE.with_borrow_mut(|s| {
        s.talking = pressed;
        if pressed {
            s.sequence = 0;
        }
    });
}

/// The mouse wheel nudges the local output volume while the key is held.
pub fn wheel(steps: f32) -> bool {
    STATE.with_borrow_mut(|s| {
        if !s.talking {
            return false;
        }
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
        s.playback.clear();
        s.sequence = 0;
        s.tick = 0;
        // `capture_on` stays as it was: the frame helper alone switches the
        // device, so it can turn a still-enabled device off after a close.
    });
}

/// Called when a world closes: drop the voice state and stop transmitting.
pub fn closed() {
    STATE.with_borrow_mut(|s| {
        s.players.clear();
        s.groups.clear();
        s.queued_packets.clear();
        s.playback.clear();
        s.talking = false;
        s.sequence = 0;
        s.tick = 0;
        // `capture_on` is not reset here either: the frame helper stops a
        // still-enabled capture device on its next frame.
    });
}

/// Queue a voice packet received from a client this tick, for the server mix.
/// Past `MAX_QUEUED_PACKETS` in one tick the new ones are dropped: a talker
/// faster than the mix is noise, not voice.
pub fn queue_packet(uuid: u128, data: Vec<u8>) {
    STATE.with_borrow_mut(|s| {
        if s.queued_packets.len() < MAX_QUEUED_PACKETS {
            s.queued_packets.push((uuid, data));
        }
    });
}

/// Drain and return the packets queued this tick.
pub fn drain_packets() -> Vec<(u128, Vec<u8>)> {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.queued_packets))
}

/// Queue a sound packet received from the server, for playback. Past
/// `MAX_PLAYBACK_PACKETS` the new ones are dropped.
pub fn queue_playback(packet: SoundPacket) {
    STATE.with_borrow_mut(|s| {
        if s.playback.len() < MAX_PLAYBACK_PACKETS {
            s.playback.push(packet);
        }
    });
}

/// Drain and return the sound packets queued for playback.
pub fn drain_playback() -> Vec<SoundPacket> {
    STATE.with_borrow_mut(|s| std::mem::take(&mut s.playback))
}

/// The local player's next outgoing frame number, counting from each hold of
/// the key. A `u32` at 50 frames a second wraps every 2.4 years of talking.
pub fn next_sequence() -> u32 {
    STATE.with_borrow_mut(|s| {
        let n = s.sequence;
        s.sequence = s.sequence.wrapping_add(1);
        n
    })
}

/// Whether the capture device is enabled right now (what the last frame
/// helper left it at), so it is only switched on a change.
pub fn capture_on() -> bool {
    STATE.with_borrow(|s| s.capture_on)
}

/// Records what the capture device is at; the frame helper calls this.
pub fn note_capture(on: bool) {
    STATE.with_borrow_mut(|s| s.capture_on = on);
}

/// One non-paused frame passed on the window: the client's clock for the
/// talking indicators' fade.
pub fn tick_local() -> u64 {
    STATE.with_borrow_mut(|s| {
        s.tick = s.tick.saturating_add(1);
        s.tick
    })
}

/// One server tick passed: the server's clock for `tick_players`.
pub fn tick_server() -> u64 {
    tick_local()
}

/// Whether a listener hears a speaker: the same dimension, and within the
/// distance their settings give a whisper or ordinary speech.
pub fn routes(
    listener_dimension: &str,
    listener_position: [f64; 3],
    speaker_dimension: &str,
    speaker_position: [f64; 3],
    whisper: bool,
    settings: &Settings,
) -> bool {
    if listener_dimension != speaker_dimension {
        return false;
    }
    let distance = if whisper {
        settings.whisper_distance
    } else {
        settings.distance
    };
    in_range(listener_position, speaker_position, distance)
}

/// The broadcast range, clamped so nearby players always hear.
pub fn broadcast_range(distance: f32) -> f32 {
    distance.clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// PCM samples as the audio device takes them.
pub fn pcm_bytes(samples: &[i16]) -> Vec<u8> {
    let mut out = Vec::with_capacity(samples.len() * 2);
    for s in samples {
        out.extend_from_slice(&s.to_le_bytes());
    }
    out
}

/// The 16-bit little-endian samples of PCM bytes; a trailing odd byte,
/// which no valid frame has, is ignored.
pub fn pcm_samples(data: &[u8]) -> Vec<i16> {
    let (chunks, _) = data.as_chunks::<2>();
    chunks.iter().map(|c| i16::from_le_bytes(*c)).collect()
}

/// How loud the PCM is, 0.0..=1.0: the root mean square of the samples.
/// Real audio only; no frame, no level.
pub fn pcm_level(data: &[u8]) -> f32 {
    let samples = pcm_samples(data);
    if samples.is_empty() {
        return 0.0;
    }
    let sum: f64 = samples
        .iter()
        .map(|&s| f64::from(s) / 32_768.0)
        .map(|v| v * v)
        .sum();
    (sum / samples.len() as f64).sqrt() as f32
}

/// A wire string: UTF-8 and at most `MAX_STRING_BYTES`, else `None`.
fn wire_string(data: &[u8]) -> Option<String> {
    if data.len() > MAX_STRING_BYTES {
        return None;
    }
    std::str::from_utf8(data).ok().map(str::to_owned)
}

/// Encodes a mic packet: version, flags, sequence, frame length, PCM.
pub fn encode_mic_packet(packet: &MicPacket) -> Result<Vec<u8>, &'static str> {
    if packet.group.is_some() {
        return Err("speak: groups are not in this version");
    }
    if packet.data.len() != MAX_FRAME_BYTES {
        return Err("speak: a PCM frame must contain exactly 960 samples");
    }
    if packet.data.len() > MAX_FRAME_BYTES {
        return Err("speak: a PCM frame is at most MAX_FRAME_BYTES");
    }
    let mut out = Vec::with_capacity(8 + packet.data.len());
    out.push(WIRE_VERSION);
    out.push(u8::from(packet.whisper));
    out.extend_from_slice(&packet.sequence.to_le_bytes());
    out.extend_from_slice(&(packet.data.len() as u16).to_le_bytes());
    out.extend_from_slice(&packet.data);
    Ok(out)
}

/// Decodes a mic packet: version, flags, sequence, frame length, PCM. A
/// packet that ends early, carries a byte past its frame, sets a flag this
/// version does not have (a group), or carries more than a frame is rejected.
pub fn decode_mic_packet(data: &[u8]) -> Option<MicPacket> {
    if data.len() < 8 || data[0] != WIRE_VERSION {
        return None;
    }
    let flags = data[1];
    if flags & !0b0000_0001 != 0 {
        return None; // a group bit or an unknown one
    }
    let sequence = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
    let len = u16::from_le_bytes([data[6], data[7]]) as usize;
    if len != MAX_FRAME_BYTES || data.len() != 8 + len {
        return None; // truncated, trailing or oversized
    }
    Some(MicPacket {
        data: data[8..].to_vec(),
        sequence,
        whisper: flags & 0b1 != 0,
        group: None,
    })
}

/// Encodes a sound packet: version, flags, sequence, sender, name,
/// dimension, position, frame length, PCM.
pub fn encode_sound_packet(packet: &SoundPacket) -> Result<Vec<u8>, &'static str> {
    if packet.data.len() != MAX_FRAME_BYTES {
        return Err("speak: a PCM frame must contain exactly 960 samples");
    }
    if packet.data.len() > MAX_FRAME_BYTES {
        return Err("speak: a PCM frame is at most MAX_FRAME_BYTES");
    }
    if !packet.position.iter().all(|c| c.is_finite()) {
        return Err("speak: a position is not finite");
    }
    if packet.sender_name.len() > MAX_STRING_BYTES || packet.dimension.len() > MAX_STRING_BYTES {
        return Err("speak: a name or dimension is too long");
    }
    let mut out = Vec::with_capacity(44 + packet.sender_name.len() + packet.dimension.len());
    out.push(WIRE_VERSION);
    out.push(0);
    out.extend_from_slice(&packet.sequence.to_le_bytes());
    out.extend_from_slice(&packet.sender.to_le_bytes());
    out.extend_from_slice(&(packet.sender_name.len() as u16).to_le_bytes());
    out.extend_from_slice(packet.sender_name.as_bytes());
    out.extend_from_slice(&(packet.dimension.len() as u16).to_le_bytes());
    out.extend_from_slice(packet.dimension.as_bytes());
    for c in packet.position {
        out.extend_from_slice(&c.to_le_bytes());
    }
    out.extend_from_slice(&(packet.data.len() as u32).to_le_bytes());
    out.extend_from_slice(&packet.data);
    Ok(out)
}

/// Decodes a sound packet, rejecting the malformed: a wrong version, a set
/// flag, a name or dimension that is not UTF-8 or too long, a position that
/// is not finite, a frame that is truncated, odd, oversized, or trails.
pub fn decode_sound_packet(data: &[u8]) -> Option<SoundPacket> {
    if data.len() < 2 + 4 + 16 || data[0] != WIRE_VERSION || data[1] != 0 {
        return None;
    }
    let sequence = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
    let sender = u128::from_le_bytes(data[6..22].try_into().ok()?);
    let mut at = 22usize;
    fn take<'a>(at: &mut usize, n: usize, data: &'a [u8]) -> Option<&'a [u8]> {
        let slice = data.get(*at..*at + n)?;
        *at += n;
        Some(slice)
    }
    let name_len = u16::from_le_bytes(take(&mut at, 2, data)?.try_into().ok()?) as usize;
    let name = wire_string(take(&mut at, name_len, data)?)?;
    let dim_len = u16::from_le_bytes(take(&mut at, 2, data)?.try_into().ok()?) as usize;
    let dimension = wire_string(take(&mut at, dim_len, data)?)?;
    let mut position = [0.0; 3];
    for c in &mut position {
        let bytes: [u8; 8] = take(&mut at, 8, data)?.try_into().ok()?;
        *c = f64::from_le_bytes(bytes);
        if !c.is_finite() {
            return None;
        }
    }
    let data_len = u32::from_le_bytes(take(&mut at, 4, data)?.try_into().ok()?) as usize;
    let pcm = take(&mut at, data_len, data)?;
    if at != data.len() || data_len != MAX_FRAME_BYTES {
        return None; // trailing, truncated, odd or oversized
    }
    Some(SoundPacket {
        sender,
        sender_name: name,
        dimension,
        position,
        sequence,
        data: pcm.to_vec(),
    })
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
        assert!((s.whisper_distance - (DEFAULT_WHISPER_DISTANCE + 5.0)).abs() < f32::EPSILON);
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

    #[test]
    fn queues_are_bounded() {
        for i in 0..(MAX_QUEUED_PACKETS + 10) {
            queue_packet(1, vec![i as u8]);
        }
        assert_eq!(drain_packets().len(), MAX_QUEUED_PACKETS);
        assert!(drain_packets().is_empty());

        for _ in 0..(MAX_PLAYBACK_PACKETS + 10) {
            queue_playback(test_sound());
        }
        assert_eq!(drain_playback().len(), MAX_PLAYBACK_PACKETS);
        assert!(drain_playback().is_empty());
    }

    #[test]
    fn mic_packet_roundtrips_on_the_wire() {
        let packet = MicPacket {
            data: pcm_bytes(&[0, 1000, -1000, i16::MAX, i16::MIN].repeat(192)),
            sequence: 7,
            whisper: true,
            group: None,
        };
        let wire = encode_mic_packet(&packet).unwrap();
        let back = decode_mic_packet(&wire).unwrap();
        assert_eq!(back.data, packet.data);
        assert_eq!(back.sequence, 7);
        assert!(back.whisper);
        assert_eq!(back.group, None);
    }

    #[test]
    fn mic_packet_refuses_groups_and_bad_frames() {
        let mut packet = test_mic();
        packet.group = Some(42);
        assert!(encode_mic_packet(&packet).is_err());

        let mut packet = test_mic();
        packet.data = vec![1]; // odd byte count
        assert!(encode_mic_packet(&packet).is_err());
        packet.data = vec![0; MAX_FRAME_BYTES + 2];
        assert!(encode_mic_packet(&packet).is_err());
    }

    #[test]
    fn mic_packet_rejects_malformed_wire() {
        let wire = encode_mic_packet(&test_mic()).unwrap();
        assert!(decode_mic_packet(&wire[..wire.len() - 1]).is_none()); // truncated
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(decode_mic_packet(&trailing).is_none()); // trailing

        let mut bad_version = wire.clone();
        bad_version[0] = WIRE_VERSION + 1;
        assert!(decode_mic_packet(&bad_version).is_none());

        let mut group_flag = wire.clone();
        group_flag[1] |= 0b10;
        assert!(decode_mic_packet(&group_flag).is_none());

        // Oversized and odd frame lengths in the header.
        let mut oversized = wire.clone();
        oversized[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(decode_mic_packet(&oversized).is_none());
        let mut odd = encode_mic_packet(&MicPacket {
            data: vec![0; 1_920],
            sequence: 0,
            whisper: false,
            group: None,
        })
        .unwrap();
        // Patch an odd length into the header directly: the encoder refuses it.
        odd[6..8].copy_from_slice(&3u16.to_le_bytes());
        assert!(decode_mic_packet(&odd).is_none());

        assert!(decode_mic_packet(&[]).is_none());
        assert!(decode_mic_packet(&[WIRE_VERSION, 0, 0, 0, 0, 0]).is_none());
    }

    #[test]
    fn sound_packet_roundtrips_on_the_wire() {
        let packet = test_sound();
        let wire = encode_sound_packet(&packet).unwrap();
        let back = decode_sound_packet(&wire).unwrap();
        assert_eq!(back.sender, packet.sender);
        assert_eq!(back.sender_name, packet.sender_name);
        assert_eq!(back.dimension, packet.dimension);
        assert_eq!(back.position, packet.position);
        assert_eq!(back.sequence, packet.sequence);
        assert_eq!(back.data, packet.data);
    }

    #[test]
    fn sound_packet_rejects_malformed_wire() {
        let wire = encode_sound_packet(&test_sound()).unwrap();
        assert!(decode_sound_packet(&wire[..wire.len() - 1]).is_none()); // truncated
        let mut trailing = wire.clone();
        trailing.push(0);
        assert!(decode_sound_packet(&trailing).is_none()); // trailing
        let mut bad_version = wire.clone();
        bad_version[0] = WIRE_VERSION + 1;
        assert!(decode_sound_packet(&bad_version).is_none());
        let mut bad_flag = wire.clone();
        bad_flag[1] = 1;
        assert!(decode_sound_packet(&bad_flag).is_none());

        // A position that is not finite.
        let mut nan = wire.clone();
        let at = 22 + 2 + test_sound().sender_name.len() + 2 + test_sound().dimension.len();
        nan[at..at + 8].copy_from_slice(&f64::NAN.to_le_bytes());
        assert!(decode_sound_packet(&nan).is_none());
        let mut inf = wire.clone();
        inf[at..at + 8].copy_from_slice(&f64::INFINITY.to_le_bytes());
        assert!(decode_sound_packet(&inf).is_none());

        // The encoder refuses the same.
        let mut packet = test_sound();
        packet.position[1] = f64::NAN;
        assert!(encode_sound_packet(&packet).is_err());

        // A frame length past the end or past the cap.
        let mut oversized = wire.clone();
        let len_at = wire.len() - packet.data.len() - 4;
        oversized[len_at..len_at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_sound_packet(&oversized).is_none());

        // Names of every length up to the cap encode; past it they do not.
        let mut packet = test_sound();
        packet.sender_name = "Üü".repeat(MAX_STRING_BYTES / 4); // 32 UTF-8 bytes
        assert!(encode_sound_packet(&packet).is_ok());
        packet.sender_name = "a".repeat(MAX_STRING_BYTES + 1);
        assert!(encode_sound_packet(&packet).is_err());

        assert!(decode_sound_packet(&[]).is_none());
    }

    #[test]
    fn routes_only_same_dimension_and_range() {
        let settings = Settings::default();
        let near = [0.0, 64.0, 0.0];
        let far = [500.0, 64.0, 500.0];
        let speaker = [10.0, 64.0, 0.0];
        assert!(routes(
            "minecraft:overworld",
            near,
            "minecraft:overworld",
            speaker,
            false,
            &settings
        ));
        assert!(!routes(
            "minecraft:the_nether",
            near,
            "minecraft:overworld",
            speaker,
            false,
            &settings
        ));
        assert!(!routes(
            "minecraft:overworld",
            far,
            "minecraft:overworld",
            speaker,
            false,
            &settings
        ));

        // A whisper carries the whisper distance, not the voice distance.
        let at = [40.0, 64.0, 0.0]; // 30 blocks: past 12, inside 100
        assert!(routes(
            "minecraft:overworld",
            at,
            "minecraft:overworld",
            speaker,
            false,
            &settings
        ));
        assert!(!routes(
            "minecraft:overworld",
            at,
            "minecraft:overworld",
            speaker,
            true,
            &settings
        ));
    }

    #[test]
    fn ptt_and_lifecycle_reset_the_client_state() {
        let none = stored(&[]);
        key(true, true, &none);
        assert!(talking());
        assert_eq!(next_sequence(), 0);
        assert_eq!(next_sequence(), 1);
        key(true, false, &none);
        assert!(!talking());

        // A new hold starts the numbers over.
        key(true, true, &none);
        assert_eq!(next_sequence(), 0);

        queue_playback(test_sound());
        queue_packet(1, vec![1]);
        note_capture(true);
        tick_local();
        closed();
        assert!(!talking());
        assert!(players().is_empty());
        assert!(drain_playback().is_empty());
        assert!(drain_packets().is_empty());
        // The frame helper, not the lifecycle, stops a still-enabled capture.
        assert!(capture_on());
        assert_eq!(next_sequence(), 0); // the counter starts over too

        // And so does a world opening.
        queue_packet(1, vec![2]);
        opened();
        assert!(drain_packets().is_empty());
    }

    #[test]
    fn ticks_count_and_feed_the_fade() {
        opened();
        assert_eq!(tick_server(), 1);
        assert_eq!(tick_server(), 2);
        assert_eq!(tick_local(), 3); // the same clock on the client's thread

        heard(
            9,
            "Beep".to_owned(),
            "minecraft:overworld".to_owned(),
            [0.0; 3],
            0.5,
            3,
        );
        assert!(players().contains_key(&9));
        tick_players(3 + TALKING_FADE_TICKS + 1);
        assert!(players().is_empty());
    }

    #[test]
    fn pcm_helpers_roundtrip_and_measure() {
        let samples: Vec<i16> = vec![0, 4096, -4096, i16::MAX];
        assert_eq!(pcm_samples(&pcm_bytes(&samples)), samples);

        assert_eq!(pcm_level(&[]), 0.0);
        assert_eq!(pcm_level(&pcm_bytes(&[0; 960])), 0.0); // silence
        let loud = pcm_level(&pcm_bytes(&[16_384; 960])); // half scale
        assert!(loud > 0.4 && loud < 0.6, "{loud}");
        let clamped = pcm_level(&pcm_bytes(&[i16::MIN; 960]));
        assert!((0.0..=1.0).contains(&clamped));
    }

    #[test]
    fn settings_reload_reaches_routing_and_wheel_requires_ptt() {
        set_settings(Settings::default());
        let value = refresh_settings(|key| (key == "distance").then(|| "12".into()));
        assert_eq!(value.distance, 12.0);
        assert!(!routes(
            "minecraft:overworld",
            [13.0, 0.0, 0.0],
            "minecraft:overworld",
            [0.0; 3],
            false,
            &value
        ));
        key(true, false, |_| None);
        assert!(!wheel(1.0));
        key(true, true, |_| None);
        assert!(wheel(1.0));
        key(true, false, |_| None);
        assert!(!wheel(1.0));
    }

    #[test]
    fn incomplete_pcm_is_not_a_voice_frame() {
        for data in [vec![], vec![0; 2], vec![0; 1918]] {
            let mut mic = test_mic();
            mic.data = data.clone();
            let mut sound = test_sound();
            sound.data = data.clone();
            assert!(encode_mic_packet(&mic).is_err());
            assert!(encode_sound_packet(&sound).is_err());
            let mut wire = encode_mic_packet(&test_mic()).unwrap();
            wire.truncate(8);
            wire[6..8].copy_from_slice(&(data.len() as u16).to_le_bytes());
            wire.extend_from_slice(&data);
            assert!(decode_mic_packet(&wire).is_none());
        }
    }

    fn test_mic() -> MicPacket {
        MicPacket {
            data: pcm_bytes(&[100, -100, 200, -200].repeat(240)),
            sequence: 1,
            whisper: false,
            group: None,
        }
    }

    fn test_sound() -> SoundPacket {
        SoundPacket {
            sender: 0x0123_4567_89ab_cdef_0123_4567_89ab_cdef,
            sender_name: "Alex".to_owned(),
            dimension: "minecraft:overworld".to_owned(),
            position: [1.0, 64.0, -2.5],
            sequence: 5,
            data: pcm_bytes(&[1, 2, 3, 4].repeat(240)),
        }
    }
}
