use std::collections::HashMap;

/// The keep-alive interval in ms (`SERVER_CONFIG.keep_alive`, default
/// 1000): the server sends a keep-alive this often.
pub const KEEP_ALIVE_MS: u64 = 1_000;
/// The drop multiplier (`sendKeepAlives`): a session whose last answer is
/// `keep_alive * 10` old is dropped — 10 s by default.
pub const TIMEOUT_MULTIPLIER: u32 = 10;

/// A player's voice session (`ClientConnection`): when they last answered
/// a keep-alive, and whether the handshake finished (`isInitialized`).
#[derive(Clone, Copy, Debug)]
pub struct Session {
    pub last_keep_alive_ms: u64,
    pub initialized: bool,
}

#[derive(Default)]
pub struct Sessions {
    sessions: HashMap<u128, Session>,
}/// What the sweep decided (`sendKeepAlives`'s removal branch).
#[derive(Debug, PartialEq)]
pub struct Sweep {
    /// The sessions the keep-alives go to (`for connection in
    /// connections.values(): sendPacket(new KeepAlivePacket(), ...)`).
    pub keep_alive_to: Vec<u128>,
    /// The sessions dropped for silence, in ms-since-answer order.
    pub dropped: Vec<u128>,
}

impl Sessions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn open(&mut self, player: u128, now_ms: u64) {
        self.sessions.insert(
            player,
            Session {
                last_keep_alive_ms: now_ms,
                initialized: false,
            },
        );
    }

    pub fn initialize(&mut self, player: u128) {
        if let Some(s) = self.sessions.get_mut(&player) {
            s.initialized = true;
        }
    }

    /// Whether the session is up and initialized (`isInitialized`).
    pub fn is_initialized(&self, player: u128) -> bool {
        self.sessions
            .get(&player)
            .is_some_and(|s| s.initialized)
    }

    pub fn answered(&mut self, player: u128, now_ms: u64) {
        if let Some(s) = self.sessions.get_mut(&player) {
            s.last_keep_alive_ms = now_ms;
        }
    }

    /// Whether the player has a session (`getConnections().containsKey`).
    pub fn has(&self, player: u128) -> bool {
        self.sessions.contains_key(&player)
    }

    /// Closes a session (`disconnectClient`: the connection and its secret
    /// go; on Horizon only the session row goes).
    pub fn close(&mut self, player: u128) {
        self.sessions.remove(&player);
    }

    /// The keep-alive sweep (`sendKeepAlives`): every live session gets a
    /// keep-alive; those silent for `keep_alive * 10` are dropped. The
    /// stamp is `now_ms`, the config's `keep_alive_ms` the caller passes.
    pub fn sweep(&mut self, now_ms: u64, keep_alive_ms: u64) -> Sweep {
        let timeout = keep_alive_ms.saturating_mul(u64::from(TIMEOUT_MULTIPLIER));
        let mut dropped: Vec<u128> = self
            .sessions
            .iter()
            .filter(|(_, s)| now_ms.saturating_sub(s.last_keep_alive_ms) >= timeout)
            .map(|(&p, _)| p)
            .collect();
        dropped.sort_unstable();
        for p in &dropped {
            self.sessions.remove(p);
        }
        Sweep {
            keep_alive_to: self.sessions.keys().copied().collect(),
            dropped,
        }
    }

    /// Every session's player, in table order (`getConnections`).
    pub fn all(&self) -> Vec<u128> {
        self.sessions.keys().copied().collect()
    }

    /// Resets everything (the world closed; `connectionSecrets.clear()`).
    pub fn reset(&mut self) {
        self.sessions.clear();
    }
}

static QUEUED: std::sync::Mutex<Vec<(u128, Vec<u8>)>> = std::sync::Mutex::new(Vec::new());

/// Queue a voice packet received from a client this tick.
pub fn queue_packet(player: u128, data: Vec<u8>) {
    if let Ok(mut queue) = QUEUED.lock() {
        queue.push((player, data));
    }
}

/// Drain and return the packets queued this tick.
pub fn drain_packets() -> Vec<(u128, Vec<u8>)> {
    if let Ok(mut queue) = QUEUED.lock() {
        std::mem::take(&mut *queue)
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_session_opens_uninitialized_and_initializes_once() {
        let mut s = Sessions::new();
        s.open(1, 0);
        assert!(s.has(1));
        assert!(!s.is_initialized(1), "the handshake pends");
        s.initialize(1);
        assert!(s.is_initialized(1));
    }

    #[test]
    fn the_sweep_drops_after_ten_keep_alives() {
        let mut s = Sessions::new();
        s.open(1, 0);
        s.initialize(1);
        // Nine seconds of silence: still held.
        let sweep = s.sweep(9_000, KEEP_ALIVE_MS);
        assert!(sweep.dropped.is_empty());
        assert_eq!(sweep.keep_alive_to, vec![1]);
        // At ten seconds the session goes.
        let sweep = s.sweep(10_000, KEEP_ALIVE_MS);
        assert_eq!(sweep.dropped, vec![1]);
        assert!(!s.has(1));
    }

    #[test]
    fn an_answer_defers_the_drop() {
        let mut s = Sessions::new();
        s.open(1, 0);
        s.initialize(1);
        s.answered(1, 9_500);
        let sweep = s.sweep(10_000, KEEP_ALIVE_MS);
        assert!(sweep.dropped.is_empty(), "answered 500 ms ago");
        let sweep = s.sweep(19_500, KEEP_ALIVE_MS);
        assert_eq!(sweep.dropped, vec![1], "ten seconds after the answer");
    }

    #[test]
    fn a_reopened_session_starts_its_clock_over() {
        let mut s = Sessions::new();
        s.open(1, 0);
        s.initialize(1);
        let sweep = s.sweep(10_000, KEEP_ALIVE_MS);
        assert_eq!(sweep.dropped, vec![1]);
        s.open(1, 10_000);
        assert!(!s.is_initialized(1), "the new session's handshake pends");
        let sweep = s.sweep(11_000, KEEP_ALIVE_MS);
        assert!(sweep.dropped.is_empty());
    }

    #[test]
    fn a_long_timeout_config_waits_longer() {
        let mut s = Sessions::new();
        s.open(1, 0);
        let sweep = s.sweep(9_000, 2_000);
        assert!(sweep.dropped.is_empty(), "2 s keep-alive: 20 s to drop");
        let sweep = s.sweep(20_000, 2_000);
        assert_eq!(sweep.dropped, vec![1]);
    }

    #[test]
    fn close_removes_without_the_sweep() {
        let mut s = Sessions::new();
        s.open(1, 0);
        s.close(1);
        assert!(!s.has(1));
        assert!(s.sweep(10_000, KEEP_ALIVE_MS).dropped.is_empty());
    }

    #[test]
    fn reset_forgets_everything() {
        let mut s = Sessions::new();
        s.open(1, 0);
        s.reset();
        assert!(s.all().is_empty());
    }
}
