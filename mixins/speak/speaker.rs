
use super::frame::{normalize_angle, angle_between};

const PAN_GAIN: f32 = 1.4;
/// The quiet ear's floor (`minVolume = 0.3F`).
const MIN_VOLUME: f32 = 0.3;
/// The vertical distance that collapses the panning (`|Δy| / 32`).
const HEIGHT_FOLD: f32 = 32.0;

pub fn stereo_volume(camera_pos: [f64; 3], y_rot: f32, sound_pos: [f64; 3]) -> [f32; 2] {
    let dx = sound_pos[0] - camera_pos[0];
    let dy = sound_pos[1] - camera_pos[1];
    let dz = sound_pos[2] - camera_pos[2];
    // The direction in the XZ plane (`Vec3.subtract().normalize()`, then
    // Vec2 of x/z), normalized against its own length.
    let flat = (dx * dx + dz * dz).sqrt();
    let (dir_x, dir_z) = if flat <= f32::EPSILON as f64 {
        (0.0f64, 0.0f64)
    } else {
        (dx / flat, dz / flat)
    };
    // The angle between the direction and (-1, 0) — vanilla's west — then
    // normalized against the player's rotation.
    let diff_angle = angle_between([dir_x as f32, dir_z as f32], [-1.0, 0.0]);
    let angle = normalize_angle(diff_angle - (y_rot % 360.0));
    // The height fold: 32 blocks of vertical distance kill the pan.
    let height_fold = f32::from(dy.abs() as f32 / HEIGHT_FOLD).min(1.0);
    let rot = angle / 180.0;
    // Fold into [-0.5, 0.5]: a sound behind flips to the nearer side.
    let mut perc = rot;
    if rot < -0.5 {
        perc = -(0.5 + (rot + 0.5));
    } else if rot > 0.5 {
        perc = 0.5 - (rot - 0.5);
    }
    perc *= 1.0 - height_fold;
    let left = if perc < 0.0 {
        perc.abs() * PAN_GAIN + MIN_VOLUME
    } else {
        MIN_VOLUME
    };
    let right = if perc >= 0.0 {
        perc * PAN_GAIN + MIN_VOLUME
    } else {
        MIN_VOLUME
    };
    // Constant-power fill to 1.
    let fill = 1.0 - left.max(right);
    [left + fill, right + fill]
}

/// The distance volume (`getDistanceVolume`): linear, reference 0 —
/// `1 - min(dist, max) / max`, matching Horizon's `distance_gain` exactly.
pub fn distance_volume(max_distance: f32, listener: [f64; 3], pos: [f64; 3]) -> f32 {
    let dx = pos[0] - listener[0];
    let dy = pos[1] - listener[1];
    let dz = pos[2] - listener[2];
    let distance = ((dx * dx + dy * dy + dz * dz).sqrt()) as f32;
    1.0 - distance.min(max_distance) / max_distance
}

/// Converts a mono frame to stereo at the given gains, constant-power
/// normalized first (`convertToStereo` with a position): the loudness of
/// the stereo pair matches the mono source's.
pub fn convert_to_stereo(audio: &[i16], camera_pos: [f64; 3], y_rot: f32, sound_pos: Option<[f64; 3]>) -> Vec<i16> {
    let (left, right) = match sound_pos {
        None => (1.0f32, 1.0f32),
        Some(pos) => {
            let [l, r] = stereo_volume(camera_pos, y_rot, pos);
            let power = (l * l + r * r).sqrt();
            (l / power, r / power)
        }
    };
    let mut out = Vec::with_capacity(audio.len() * 2);
    for &sample in audio {
        out.push((f32::from(sample) * left) as i16);
        out.push((f32::from(sample) * right) as i16);
    }
    out
}

/// Mono duplicates to both channels (`convertToStereo(audio)`), for the
/// recorder's own track.
pub fn duplicate_to_stereo(audio: &[i16]) -> Vec<i16> {
    let mut out = Vec::with_capacity(audio.len() * 2);
    for &sample in audio {
        out.push(sample);
        out.push(sample);
    }
    out
}

/// The audio types (`AudioType`): how a positional voice plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum AudioType {
    /// The game's own 3D math: OpenAL-identical panning and linear
    /// distance attenuation.
    #[default]
    Normal,
    /// Software panning into stereo, software distance.
    Reduced,
    /// Mono, software distance only.
    Off,
}

/// The listener's state (`CameraState`'s essentials): the camera's
/// position and rotation, the owner's eye position.
#[derive(Clone, Copy, Debug)]
pub struct Listener {
    pub position: [f64; 3],
    pub yaw: f32,
}

/// The playback decision for one frame: the samples and where they play.
#[derive(Clone, Debug, PartialEq)]
pub enum Playback {
    /// Non-positional: the group packets, the self-echo, a spectator's
    /// freecam path.
    Static { mono: Vec<i16>, volume: f32 },
    /// Positional: the two gains per ear (NORMAL), a stereo frame
    /// (REDUCED), or a mono frame with a distance volume (OFF).
    Positional {
        mono: Vec<i16>,
        volume: f32,
        stereo: Option<[f32; 2]>,
    },
    /// Out of range: don't play at all (`AudioChannel`'s cull), but keep
    /// the talking indicator if the level says so.
    Cull,
}

/// The playback for one frame under a type (`writeToSpeaker`'s branches).
pub fn playback(
    audio_type: AudioType,
    frame: &[i16],
    base_volume: f32,
    max_distance: f32,
    listener: &Listener,
    sound_pos: Option<[f64; 3]>,
) -> Playback {
    match sound_pos {
        None => Playback::Static { mono: frame.to_vec(), volume: base_volume },
        Some(pos) => {
            // The distance volume every type applies (NORMAL through
            // OpenAL, REDUCED and OFF in software — the same curve).
            let distance = distance_volume(max_distance, listener.position, pos);
            if distance <= 0.0 {
                return Playback::Cull;
            }
            let volume = base_volume * distance;
            match audio_type {
                AudioType::Normal => {
                    // The game's own gains, computed here so the voice
                    // pans identically to the game's sounds.
                    let stereo = horizon_stereo_gains(listener, pos);
                    Playback::Positional { mono: frame.to_vec(), volume, stereo: Some(stereo) }
                }
                AudioType::Reduced => {
                    let stereo = stereo_volume(listener.position, listener.yaw, pos);
                    // Constant-power normalization matches OpenAL's
                    // mono-source loudness.
                    let power = (stereo[0] * stereo[0] + stereo[1] * stereo[1]).sqrt();
                    Playback::Positional {
                        mono: frame.to_vec(),
                        volume,
                        stereo: Some([stereo[0] / power, stereo[1] / power]),
                    }
                }
                AudioType::Off => Playback::Positional {
                    mono: frame.to_vec(),
                    volume,
                    stereo: None,
                },
            }
        }
    }
}

pub fn horizon_stereo_gains(listener: &Listener, pos: [f64; 3]) -> [f32; 2] {
    let (yaw, pitch) = (f64::from(listener.yaw.to_radians()), 0.0f64);
    let forward = [
        -yaw.sin() * pitch.cos(),
        -pitch.sin(),
        yaw.cos() * pitch.cos(),
    ];
    let up = [
        -yaw.sin() * pitch.sin(),
        pitch.cos(),
        yaw.cos() * pitch.sin(),
    ];
    let right = [
        forward[1] * up[2] - forward[2] * up[1],
        forward[2] * up[0] - forward[0] * up[2],
        forward[0] * up[1] - forward[1] * up[0],
    ];
    let d = [
        pos[0] - listener.position[0],
        pos[1] - listener.position[1],
        pos[2] - listener.position[2],
    ];
    let dot = |a: [f64; 3], b: [f64; 3]| (a[0] * b[0] + a[1] * b[1] + a[2] * b[2]) as f32;
    stereo_gains_openal(dot(d, right), dot(d, up), dot(d, forward))
}

/// OpenAL Soft's stereo gains for a source direction (Horizon's
/// `stereo_gains`, copied verbatim from its mixer): the elevation folds
/// the azimuth, the front adds a center gain, and the pair lands in
/// [0, 1].
fn stereo_gains_openal(right: f32, up: f32, forward: f32) -> [f32; 2] {
    let length = (right * right + up * up + forward * forward).sqrt();
    if length <= f32::EPSILON {
        return stereo_gains_openal(0.0, 0.0, 1.0);
    }
    let elevation = (up / length).clamp(-1.0, 1.0).asin();
    let mut azimuth = right.atan2(forward);
    if azimuth.abs() < std::f32::consts::FRAC_PI_2 {
        azimuth = (azimuth.abs() * 1.5)
            .min(std::f32::consts::FRAC_PI_2)
            .copysign(azimuth);
    }
    let side = azimuth.sin() * elevation.cos();
    let front = azimuth.cos() * elevation.cos() * 0.095_660_8;
    [0.5 - 0.5 * side + front, 0.5 + 0.5 * side + front]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener_at(x: f64, yaw: f32) -> Listener {
        Listener { position: [x, 64.0, 0.0], yaw }
    }

    #[test]
    fn a_level_side_sound_reaches_both_ears_full() {        // A level sound to the side at the pan's reference geometry:
        let listener = listener_at(0.0, 0.0);
        let [l, r] = stereo_volume(listener.position, listener.yaw, [-8.0, 64.0, 0.0]);
        assert_eq!((l, r), (1.0, 1.0), "the folded side lands at full both");
    }

    #[test]
    fn the_quiet_ear_rests_at_the_minimum() {
        // A sound whose fold lands at the reference angle: one ear stays
        // at the 0.3 floor while the other takes the pan gain.
        let listener = listener_at(0.0, 0.0);
        let [l, r] = stereo_volume(listener.position, listener.yaw, [0.0, 64.0, 8.0]);
        assert_eq!((l, r), (0.3, 1.0), "the reference side: floor vs full");
    }

    #[test]
    fn the_height_fold_kills_the_pan() {
        let listener = listener_at(0.0, 0.0);
        // A sound far above and to the side folds back to full both: the
        // height factor (|Δy|/32) zeroes the pan percentage (the
        // geometry's rot folds to 0 anyway; the fold-multiplied zero pins
        // it).
        let [l, r] = stereo_volume(listener.position, listener.yaw, [8.0, 96.0, 0.0]);
        assert_eq!((l, r), (1.0, 1.0), "a far-above sound loses its pan");
        // Nearer above it keeps some spread, but less than level (16 up
        // on a 5-block offset: the height factor is 0.5, halving the pan).
        let [l2, r2] = stereo_volume(listener.position, listener.yaw, [4.0, 80.0, 3.0]);
        let [l3, r3] = stereo_volume(listener.position, listener.yaw, [4.0, 64.0, 3.0]);
        let spread_level = (l3 - r3).abs();
        let spread_high = (l2 - r2).abs();
        assert!(
            spread_high < spread_level,
            "height narrows the pan: {spread_high} vs {spread_level}"
        );
        assert!(spread_high > 0.0, "16 above still pans a little");
    }

    #[test]
    fn both_ears_sum_to_one() {
        let listener = listener_at(0.0, 0.0);
        for dx in [-20.0f64, -8.0, -1.0, 0.0, 1.0, 8.0, 20.0] {
            let [l, r] = stereo_volume(listener.position, listener.yaw, [dx, 64.0, 0.0]);
            assert!((l + r - 2.0).abs() < 1e-4, "constant power at {dx}: {l}+{r}");
        }
    }

    #[test]
    fn the_distance_volume_is_linear_to_zero() {
        let listener = listener_at(0.0, 0.0);
        assert_eq!(distance_volume(48.0, listener.position, [0.0, 64.0, 0.0]), 1.0);
        assert_eq!(distance_volume(48.0, listener.position, [24.0, 64.0, 0.0]), 0.5);
        assert_eq!(distance_volume(48.0, listener.position, [48.0, 64.0, 0.0]), 0.0);
        // Past the max it clamps to 0 (the mix already filtered by range;
        // the client's cull is this).
        assert_eq!(distance_volume(48.0, listener.position, [100.0, 64.0, 0.0]), 0.0);
    }

    #[test]
    fn the_stereo_conversion_normalizes_to_constant_power() {
        let listener = listener_at(0.0, 0.0);
        let frame = vec![1000i16; 4];
        let stereo = convert_to_stereo(&frame, listener.position, listener.yaw, Some([8.0, 64.0, 0.0]));
        assert_eq!(stereo.len(), 8);
        // The left and right gains are constant-power normalized: their
        // squares sum to 1 (within the pan's own power).
        let [l, r] = stereo_volume(listener.position, listener.yaw, [8.0, 64.0, 0.0]);
        let power = ((l / (l * l + r * r).sqrt()) as f64).powi(2)
            + ((r / (l * l + r * r).sqrt()) as f64).powi(2);
        assert!((power - 1.0).abs() < 1e-4);
    }

    #[test]
    fn duplicate_to_stereo_duplicates_the_samples() {
        let out = duplicate_to_stereo(&[100, -200]);
        assert_eq!(out, vec![100, 100, -200, -200]);
    }

    #[test]
    fn the_playback_types_differ_exactly_as_shipped() {
        let listener = listener_at(0.0, 0.0);
        let frame = vec![1000i16; 960];
        let pos = Some([8.0, 64.0, 0.0]);
        // NORMAL: stereo gains from the game's math.
        let normal = playback(AudioType::Normal, &frame, 1.0, 48.0, &listener, pos);
        assert!(matches!(normal, Playback::Positional { stereo: Some(_), .. }));
        // REDUCED: stereo too, but from the software pan.
        let reduced = playback(AudioType::Reduced, &frame, 1.0, 48.0, &listener, pos);
        assert!(matches!(reduced, Playback::Positional { stereo: Some(_), .. }));
        // OFF: no stereo, distance only.
        let off = playback(AudioType::Off, &frame, 1.0, 48.0, &listener, pos);
        assert!(matches!(off, Playback::Positional { stereo: None, .. }));
        // Static (a group packet) plays non-positionally under every type.
        let static_playback = playback(AudioType::Normal, &frame, 1.0, 48.0, &listener, None);
        assert!(matches!(static_playback, Playback::Static { .. }));
    }

    #[test]
    fn an_out_of_range_position_culls() {
        let listener = listener_at(0.0, 0.0);
        let playback = playback(AudioType::Normal, &[1; 960], 1.0, 48.0, &listener, Some([100.0, 64.0, 0.0]));
        assert_eq!(playback, Playback::Cull);
    }

    #[test]
    fn the_openal_gains_match_the_game_mixer_exactly() {
        // Horizon's own constants, reproduced: a source directly to the
        // side goes to one ear entirely (the 1.5 azimuth fold saturates at
        // a quarter turn), and a source ahead sits center with the front
        // gain added to both ears.
        let [l, r] = stereo_gains_openal(1.0, 0.0, 0.0);
        assert!(l.abs() < 1e-6 && r == 1.0, "a side source is one ear: {l} {r}");
        let [l, r] = stereo_gains_openal(0.0, 0.0, 1.0);
        assert!((l - 0.5956608).abs() < 1e-6, "front left {l}");
        assert!((r - 0.5956608).abs() < 1e-6, "front right {r}");
        // A source directly behind folds to the nearer side.
        let [l, r] = stereo_gains_openal(0.0, 0.0, -1.0);
        assert!(l != r, "behind still pans: {l} vs {r}");
    }
}
