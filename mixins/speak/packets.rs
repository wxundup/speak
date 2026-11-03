use super::frame::{MAX_GROUP_NAME_LENGTH, MAX_VOICE_CHAT_PACKET_SIZE};

pub const KIND_VOICE: u8 = 1;
pub const KIND_CONTROL: u8 = 2;

/// The voice frames (`MicPacket` up, the `SoundPacket` family down).
pub const VOICE_MIC: u8 = 1;
pub const VOICE_SOUND: u8 = 2;

/// The sound packet's three forms (`PlayerSoundPacket`, `GroupSoundPacket`,
/// `LocationSoundPacket`), carried in one frame's `sound_kind`.
pub const SOUND_PLAYER: u8 = 1;
pub const SOUND_GROUP: u8 = 2;
pub const SOUND_LOCATION: u8 = 3;

/// The control frames, in the order the session uses them.
pub const CONTROL_KEEPALIVE: u8 = 1;
pub const CONTROL_PING: u8 = 2;
pub const CONTROL_PONG: u8 = 3;
pub const CONTROL_STATE: u8 = 4;
pub const CONTROL_STATES: u8 = 5;
pub const CONTROL_REMOVE_STATE: u8 = 6;
pub const CONTROL_ADD_GROUP: u8 = 7;
pub const CONTROL_REMOVE_GROUP: u8 = 8;
pub const CONTROL_JOINED_GROUP: u8 = 9;
pub const CONTROL_REQUEST_STATES: u8 = 10;
pub const CONTROL_HELLO: u8 = 11;

pub const COMPATIBILITY_VERSION: u32 = 1;

/// A mic packet from a client: the encoded frame, whether the sender
/// whispers, and the frame's place in the stream. The sequence counter
/// starts at 0 and increments per sent frame, stop frames included.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MicPacket {
    pub whisper: bool,
    pub sequence_number: u64,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SoundPacket {
    pub sound_kind: u8,
    /// Who spoke; also the channel id for player and group sounds.
    pub sender: u128,
    pub channel_id: u128,
    pub sequence_number: i64,
    pub whisper: bool,
    /// The distance the voice carries (`PlayerSoundPacket.distance`).
    pub distance: f32,
    /// Where the voice came from (the sender's position; a locational
    /// sound's own).
    pub position: [f64; 3],
    /// The sender's dimension (`minecraft:overworld`), for the mix.
    pub dimension: String,
    /// The sender's name, for the HUD's talking list.
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PlayerState {
    pub uuid: u128,
    pub name: String,
    pub muted: bool,
    pub force_muted: bool,
    pub disabled: bool,
    pub force_disabled: bool,
    pub disconnected: bool,
    pub group: Option<u128>,
}

/// A voice group (`Group`, as `toClientGroup` sends it): the password
/// itself never leaves the server, only whether there is one.
#[derive(Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u128,
    pub name: String,
    pub has_password: bool,
    pub persistent: bool,
    pub hidden: bool,
    /// `0` normal, `1` open, `2` isolated (`api.Group.Type`'s order).
    pub group_type: u8,
}

// --- little-endian writers and readers -------------------------------------

fn put_u64(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_u128(out: &mut Vec<u8>, v: u128) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f32(out: &mut Vec<u8>, v: f32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_f64(out: &mut Vec<u8>, v: f64) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn put_string(out: &mut Vec<u8>, s: &str, cap: usize) {
    let bytes = s.as_bytes();
    let len = bytes.len().min(255).min(cap);
    out.push(len as u8);
    out.extend_from_slice(&bytes[..len]);
}

struct Reader<'a> {
    data: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, at: 0 }
    }

    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let end = self.at.checked_add(n)?;
        let bytes = self.data.get(self.at..end)?;
        self.at = end;
        Some(bytes)
    }

    fn u8(&mut self) -> Option<u8> {
        Some(self.take(1)?[0])
    }

    fn u64(&mut self) -> Option<u64> {
        Some(u64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn u128(&mut self) -> Option<u128> {
        Some(u128::from_le_bytes(self.take(16)?.try_into().ok()?))
    }

    fn f32(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn f64(&mut self) -> Option<f64> {
        Some(f64::from_le_bytes(self.take(8)?.try_into().ok()?))
    }

    fn string(&mut self) -> Option<String> {
        let len = usize::from(self.u8()?);
        let bytes = self.take(len)?;
        String::from_utf8(bytes.to_vec()).ok()
    }
}

// --- voice frames -----------------------------------------------------------

/// Frames a mic packet: `[KIND_VOICE][VOICE_MIC][whisper][seq u64][data…]`.
pub fn encode_mic(packet: &MicPacket) -> Vec<u8> {
    let mut out = Vec::with_capacity(19 + packet.data.len());
    out.push(KIND_VOICE);
    out.push(VOICE_MIC);
    out.push(u8::from(packet.whisper));
    put_u64(&mut out, packet.sequence_number);
    out.extend_from_slice(&packet.data);
    out
}

/// Parses a mic frame; `None` when it is not one or is truncated.
pub fn decode_mic(data: &[u8]) -> Option<MicPacket> {
    let mut r = Reader::new(data);
    if r.u8()? != KIND_VOICE {
        return None;
    }
    if r.u8()? != VOICE_MIC {
        return None;
    }
    let whisper = r.u8()? != 0;
    let sequence_number = r.u64()?;
    Some(MicPacket {
        whisper,
        sequence_number,
        data: data[r.at..].to_vec(),
    })
}

/// Frames a sound packet:
/// `[KIND_VOICE][VOICE_SOUND][sound_kind][sender u128][channel u128]
/// [seq i64][whisper][distance f32][position 3 f64][dim str][name str][data…]`.
pub fn encode_sound(packet: &SoundPacket) -> Vec<u8> {
    let mut out = Vec::with_capacity(
        2 + 1 + 16 + 16 + 8 + 1 + 4 + 24 + 2 + packet.dimension.len() + packet.name.len()
            + packet.data.len(),
    );
    out.push(KIND_VOICE);
    out.push(VOICE_SOUND);
    out.push(packet.sound_kind);
    put_u128(&mut out, packet.sender);
    put_u128(&mut out, packet.channel_id);
    put_u64(&mut out, packet.sequence_number as u64);
    out.push(u8::from(packet.whisper));
    put_f32(&mut out, packet.distance);
    for p in packet.position {
        put_f64(&mut out, p);
    }
    put_string(&mut out, &packet.dimension, 64);
    put_string(&mut out, &packet.name, 64);
    out.extend_from_slice(&packet.data);
    out
}

/// Parses a sound frame; `None` when it is not one or is truncated.
pub fn decode_sound(data: &[u8]) -> Option<SoundPacket> {
    let mut r = Reader::new(data);
    if r.u8()? != KIND_VOICE {
        return None;
    }
    if r.u8()? != VOICE_SOUND {
        return None;
    }
    let sound_kind = r.u8()?;
    let sender = r.u128()?;
    let channel_id = r.u128()?;
    let sequence_number = r.u64()? as i64;
    let whisper = r.u8()? != 0;
    let distance = r.f32()?;
    let position = [r.f64()?, r.f64()?, r.f64()?];
    let dimension = r.string()?;
    let name = r.string()?;
    Some(SoundPacket {
        sound_kind,
        sender,
        channel_id,
        sequence_number,
        whisper,
        distance,
        position,
        dimension,
        name,
        data: data[r.at..].to_vec(),
    })
}

// --- control frames ---------------------------------------------------------

/// Frames a control frame: `[KIND_CONTROL][control][payload…]`.
pub fn encode_control(control: u8, payload: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + payload.len());
    out.push(KIND_CONTROL);
    out.push(control);
    out.extend_from_slice(payload);
    out
}

/// Splits a control frame into its kind and payload; `None` for non-control
/// frames and truncations.
pub fn decode_control(data: &[u8]) -> Option<(u8, &[u8])> {
    let mut r = Reader::new(data);
    if r.u8()? != KIND_CONTROL {
        return None;
    }
    let control = r.u8()?;
    Some((control, &data[r.at..]))
}

/// Frames a player state (`PlayerState.toBytes`'s flag order).
pub fn encode_state(state: &PlayerState) -> Vec<u8> {
    let mut flags = 0u8;
    if state.muted {
        flags |= 1 << 0;
    }
    if state.force_muted {
        flags |= 1 << 1;
    }
    if state.disabled {
        flags |= 1 << 2;
    }
    if state.force_disabled {
        flags |= 1 << 3;
    }
    if state.disconnected {
        flags |= 1 << 4;
    }
    let has_group = state.group.is_some();
    if has_group {
        flags |= 1 << 5;
    }
    let mut out = Vec::with_capacity(2 + 16 + 1 + state.name.len() + 16);
    out.push(flags);
    put_u128(&mut out, state.uuid);
    put_string(&mut out, &state.name, 256);
    if let Some(group) = state.group {
        put_u128(&mut out, group);
    }
    out
}

/// Parses a player state; `None` on truncation.
pub fn decode_state(data: &[u8]) -> Option<PlayerState> {
    let mut r = Reader::new(data);
    let flags = r.u8()?;
    let mut state = PlayerState {
        uuid: r.u128()?,
        name: r.string()?,
        muted: flags & (1 << 0) != 0,
        force_muted: flags & (1 << 1) != 0,
        disabled: flags & (1 << 2) != 0,
        force_disabled: flags & (1 << 3) != 0,
        disconnected: flags & (1 << 4) != 0,
        group: None,
    };
    if flags & (1 << 5) != 0 {
        state.group = Some(r.u128()?);
    }
    Some(state)
}

/// Frames a group as the clients see it (`Group.toClientGroup`): a
/// has-password flag, never the password.
pub fn encode_group(group: &Group) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 16 + 1 + group.name.len() + 4);
    put_u128(&mut out, group.id);
    put_string(&mut out, &group.name, MAX_GROUP_NAME_LENGTH);
    out.push(u8::from(group.has_password));
    out.push(u8::from(group.persistent));
    out.push(u8::from(group.hidden));
    out.push(group.group_type);
    encode_control(CONTROL_ADD_GROUP, &out)
}

/// Parses a group frame (the `CONTROL_ADD_GROUP` wrapper is expected).
pub fn decode_group(data: &[u8]) -> Option<Group> {
    let (control, payload) = decode_control(data)?;
    if control != CONTROL_ADD_GROUP {
        return None;
    }
    let mut r = Reader::new(payload);
    let id = r.u128()?;
    let name = r.string()?;
    let has_password = r.u8()? != 0;
    let persistent = r.u8()? != 0;
    let hidden = r.u8()? != 0;
    let group_type = r.u8()?;
    if group_type > 2 {
        return None;
    }
    Some(Group {
        id,
        name,
        has_password,
        persistent,
        hidden,
        group_type,
    })
}

pub fn ttl_ticks(frame: &[u8]) -> u32 {
    if frame.first() == Some(&KIND_VOICE) {
        10
    } else {
        200
    }
}

/// Whether a frame fits the wire (`Utils.MAX_VOICE_CHAT_PACKET_SIZE`'s
/// role; Horizon's mod messages cap at 30 000, the tighter of the two here
/// is the frame's own budget).
pub fn fits_wire(frame: &[u8]) -> bool {
    frame.len() <= MAX_VOICE_CHAT_PACKET_SIZE
}

/// A command the mod adds to the server's command tree
/// (`NativeCommand`, API 8): one usage, words then `<name:kind>`
/// arguments; a usage every player may run.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NativeCommand {
    pub usage: String,
}

impl NativeCommand {
    pub fn new(usage: &str) -> Self {
        Self { usage: usage.to_owned() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mic_frame_keeps_its_field_order() {
        let packet = MicPacket {
            whisper: true,
            sequence_number: 7,
            data: vec![1, 2, 3],
        };
        let frame = encode_mic(&packet);
        assert_eq!(frame[0], KIND_VOICE);
        assert_eq!(frame[1], VOICE_MIC);
        assert_eq!(frame[2], 1, "whisper");
        assert_eq!(u64::from_le_bytes(frame[3..11].try_into().unwrap()), 7);
        assert_eq!(&frame[11..], &[1, 2, 3]);
        assert_eq!(decode_mic(&frame), Some(packet));
        // Truncations refuse.
        for cut in 0..11 {
            assert_eq!(decode_mic(&frame[..cut]), None, "cut at {cut}");
        }
        // A control frame is not a mic frame.
        assert_eq!(decode_mic(&encode_control(CONTROL_KEEPALIVE, &[])), None);
    }

    #[test]
    fn a_sound_frame_carries_every_field() {
        let packet = SoundPacket {
            sound_kind: SOUND_PLAYER,
            sender: 0xAAAA_BBBB_CCCC_1111,
            channel_id: 0xAAAA_BBBB_CCCC_1111,
            sequence_number: -5,
            whisper: true,
            distance: 48.0,
            position: [1.5, 64.0, -2.5],
            dimension: "minecraft:overworld".into(),
            name: "Alex".into(),
            data: vec![9, 9],
        };
        let frame = encode_sound(&packet);
        assert_eq!(frame[0], KIND_VOICE);
        assert_eq!(frame[1], VOICE_SOUND);
        assert_eq!(frame[2], SOUND_PLAYER);
        assert_eq!(decode_sound(&frame), Some(packet.clone()));
        // A negative sequence survives the round trip (the client-channel
        // marker), which a u64 round trip alone would not show.
        let mut packet2 = packet.clone();
        packet2.sequence_number = i64::MIN;
        assert_eq!(decode_sound(&encode_sound(&packet2)).unwrap().sequence_number, i64::MIN);
    }

    #[test]
    fn the_state_flags_match_player_states_byte_order() {
        let state = PlayerState {
            uuid: 42,
            name: "Alex".into(),
            muted: true,
            force_muted: false,
            disabled: true,
            force_disabled: true,
            disconnected: false,
            group: Some(7),
        };
        let frame = encode_state(&state);
        // Hand-built: bit 0 muted, bit 2 disabled, bit 3 force_disabled,
        // bit 5 has_group.
        assert_eq!(frame[0], 0b0010_1101);
        assert_eq!(decode_state(&frame), Some(state));
        // Every flag bit round-trips alone.
        for bit in 0..6u8 {
            let mut state = PlayerState {
                uuid: 1,
                name: "A".into(),
                ..PlayerState::default()
            };
            let frame = {
                match bit {
                    0 => state.muted = true,
                    1 => state.force_muted = true,
                    2 => state.disabled = true,
                    3 => state.force_disabled = true,
                    4 => state.disconnected = true,
                    _ => state.group = Some(9),
                }
                encode_state(&state)
            };
            assert_eq!(decode_state(&frame), Some(state), "bit {bit}");
        }
    }

    #[test]
    fn a_group_frame_never_carries_the_password() {
        let group = Group {
            id: 99,
            name: "team".into(),
            has_password: true,
            persistent: false,
            hidden: true,
            group_type: 2,
        };
        let frame = encode_group(&group);
        let text = String::from_utf8_lossy(&frame);
        assert!(!text.contains("pw"), "no password field exists at all");
        assert_eq!(decode_group(&frame), Some(group));
        let mut bad = encode_group(&Group {
            id: 1,
            name: "x".into(),
            has_password: false,
            persistent: false,
            hidden: false,
            group_type: 3,
        });
        let at = bad.len() - 1;
        bad[at] = 3;
        assert_eq!(decode_group(&bad), None);
    }

    #[test]
    fn control_frames_split_and_refuse_other_kinds() {
        let frame = encode_control(CONTROL_PING, &[1, 2, 3]);
        assert_eq!(frame[0], KIND_CONTROL);
        assert_eq!(frame[1], CONTROL_PING);
        assert_eq!(decode_control(&frame), Some((CONTROL_PING, &[1, 2, 3][..])));
        assert_eq!(decode_control(&encode_mic(&MicPacket {
            whisper: false,
            sequence_number: 0,
            data: vec![],
        })), None);
        assert_eq!(decode_control(&[]), None);
    }

    #[test]
    fn voice_frames_have_a_short_ttl_and_bookkeeping_a_long_one() {
        let mic = encode_mic(&MicPacket {
            whisper: false,
            sequence_number: 0,
            data: vec![0; 960],
        });
        assert_eq!(ttl_ticks(&mic), 10);
        assert_eq!(ttl_ticks(&encode_control(CONTROL_KEEPALIVE, &[])), 200);
    }

    #[test]
    fn frames_stay_within_the_wire_budget() {
        let mic = encode_mic(&MicPacket {
            whisper: false,
            sequence_number: 0,
            data: vec![0; MAX_VOICE_CHAT_PACKET_SIZE],
        });
        assert!(!fits_wire(&mic), "a frame past the budget refuses");
        let small = encode_mic(&MicPacket {
            whisper: false,
            sequence_number: 0,
            data: vec![0; 960],
        });
        assert!(fits_wire(&small));
    }

    #[test]
    fn strings_truncate_to_their_length_byte() {
        let state = PlayerState {
            uuid: 1,
            name: "x".repeat(300),
            ..PlayerState::default()
        };
        let frame = encode_state(&state);
        let back = decode_state(&frame).unwrap();
        assert_eq!(back.name.len(), 255, "the length byte's cap");
    }
}
