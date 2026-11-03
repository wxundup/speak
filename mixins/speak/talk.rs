use std::collections::HashMap;

/// How long a talk entry stays fresh (`TalkCache.TIMEOUT`).
pub const TIMEOUT_MS: u64 = 250;

/// The floor level (`AudioUtils.LOWEST_DB`) a stale entry reports.
pub const DEFAULT_LEVEL_DB: f64 = -127.0;

/// One player's talk state (`TalkCache.PlayerCache`): when they last
/// spoke, whether it was a whisper, and their level.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Talk {
    pub timestamp_ms: u64,
    pub whispering: bool,
    pub level_db: f64,
}

impl Default for Talk {
    fn default() -> Self {
        // `TalkCache.DEFAULT`: timestamp 0, not whispering, the floor.
        Self {
            timestamp_ms: 0,
            whispering: false,
            level_db: DEFAULT_LEVEL_DB,
        }
    }
}

#[derive(Default)]
pub struct TalkCache {
    players: HashMap<u128, Talk>,
}

impl TalkCache {
    pub fn new() -> Self {
        Self::default()
    }

    /// A frame from a player arrived: their level is the frame's peak dB,
    /// their whisper flag stands, and the clock is the caller's. An empty
    /// frame is the stop marker: the entry stays (with its level), and
    /// the freshness comes from the timestamp alone — which updates on
    /// every packet, stop included.
    pub fn update(
        &mut self,
        player: u128,
        whispering: bool,
        frame: &[i16],
        now_ms: u64,
        level_of: impl Fn(&[i16]) -> f64,
    ) {
        let level = if frame.is_empty() {
            // The level updates with the empty frame's level, which is
            // the floor.
            DEFAULT_LEVEL_DB
        } else {
            level_of(frame)
        };
        self.players.insert(
            player,
            Talk {
                timestamp_ms: now_ms,
                whispering,
                level_db: level,
            },
        );
    }

    pub fn is_talking(&self, player: u128, now_ms: u64) -> bool {
        self.players
            .get(&player)
            .is_some_and(|t| now_ms.saturating_sub(t.timestamp_ms) < TIMEOUT_MS)
    }

    /// Whether the player's last frame was a whisper (`isWhispering`).
    pub fn is_whispering(&self, player: u128, now_ms: u64) -> bool {
        self.players
            .get(&player)
            .filter(|t| now_ms.saturating_sub(t.timestamp_ms) < TIMEOUT_MS)
            .is_some_and(|t| t.whispering)
    }

    /// The player's audio level (`getPlayerAudioLevel`): the stored level
    /// while fresh, the floor after the timeout.
    pub fn level_db(&self, player: u128, now_ms: u64) -> f64 {
        self.players
            .get(&player)
            .filter(|t| now_ms.saturating_sub(t.timestamp_ms) < TIMEOUT_MS)
            .map_or(DEFAULT_LEVEL_DB, |t| t.level_db)
    }

    /// Everyone fresh, for the HUD's talking list (the callers iterate the
    /// online players and ask; this is the reverse map).
    pub fn talking_players(&self, now_ms: u64) -> Vec<u128> {
        self.players
            .iter()
            .filter(|(_, t)| now_ms.saturating_sub(t.timestamp_ms) < TIMEOUT_MS)
            .map(|(&p, _)| p)
            .collect()
    }

    /// Forgets a player (their state went away).
    pub fn remove(&mut self, player: u128) {
        self.players.remove(&player);
    }

    /// Resets everything (the world closed).
    pub fn reset(&mut self) {
        self.players.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn level_peak(frame: &[i16]) -> f64 {
        frame.iter().fold(0.0, |m, &s| m.max(f64::from(s.abs())))
    }

    #[test]
    fn a_frame_makes_the_player_talk_for_250_ms() {
        let mut c = TalkCache::new();
        c.update(1, false, &[1000], 1000, level_peak);
        assert!(c.is_talking(1, 1000));
        assert!(c.is_talking(1, 1249), "the last fresh ms");
        assert!(!c.is_talking(1, 1250), "timeout at 250");
        assert_eq!(c.level_db(1, 1250), DEFAULT_LEVEL_DB, "stale is the floor");
    }

    #[test]
    fn the_whisper_flag_stands_while_fresh() {
        let mut c = TalkCache::new();
        c.update(1, true, &[1000], 1000, level_peak);
        assert!(c.is_whispering(1, 1249));
        assert!(!c.is_whispering(1, 1250));
    }

    #[test]
    fn the_level_is_the_frames_peak() {
        let mut c = TalkCache::new();
        c.update(1, false, &[100, 3000, -200], 0, level_peak);
        let level = c.level_db(1, 10);
        assert!(level > -60.0, "3000 is loud: {level}");
    }

    #[test]
    fn an_empty_frame_updates_the_freshness_at_the_floor_level() {
        let mut c = TalkCache::new();
        c.update(1, false, &[3000], 0, level_peak);
        c.update(1, false, &[], 100, level_peak);
        // The stop marker refreshes the timestamp, and the level reads
        // the floor (an empty frame's peak).
        assert!(c.is_talking(1, 300));
        assert_eq!(c.level_db(1, 300), DEFAULT_LEVEL_DB);
    }

    #[test]
    fn a_stale_player_is_forgotten_from_the_list() {
        let mut c = TalkCache::new();
        c.update(1, false, &[1], 0, level_peak);
        c.update(2, false, &[1], 300, level_peak);
        assert_eq!(c.talking_players(350), vec![2]);
        assert_eq!(c.talking_players(600), Vec::<u128>::new());
    }

    #[test]
    fn a_removal_and_a_reset_clear_the_entries() {
        let mut c = TalkCache::new();
        c.update(1, false, &[1], 0, level_peak);
        c.remove(1);
        assert!(!c.is_talking(1, 10));
        c.update(1, false, &[1], 0, level_peak);
        c.reset();
        assert!(!c.is_talking(1, 10));
    }

    #[test]
    fn an_unknown_player_defaults_to_the_floor() {
        let c = TalkCache::new();
        assert!(!c.is_talking(9, 0));
        assert_eq!(c.level_db(9, 0), DEFAULT_LEVEL_DB);
        assert!(!c.is_whispering(9, 0));
    }
}
