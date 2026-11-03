use std::collections::HashMap;

use super::packets::{CONTROL_PING, CONTROL_PONG};

/// What a ping's listener hears (`PingManager.PingListener`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PingOutcome {
    /// A pong after `attempts` attempts, `ping_ms` round trip
    /// (`onPong`; attempts 1 is a clean answer).
    Pong { attempts: u32, ping_ms: u64 },
    /// An attempt timed out and another goes out (`onFailedAttempt`).
    Retry { attempts: u32 },
    /// All attempts spent (`onTimeout`).
    Timeout { attempts: u32 },
}

/// A ping in flight (`PingManager.Ping`): its id, when the attempt went
/// out, its timeout, and how many attempts remain.
struct Ping {
    timeout_ms: u64,
    max_attempts: u32,
    attempts: u32,
    started_ms: u64,
}

#[derive(Default)]
pub struct PingManager {
    in_flight: HashMap<u128, Ping>,
}

/// A ping to send: the frame the caller transmits, and the id to answer
/// it with.
pub struct Outgoing {
    pub id: u128,
    /// The `CONTROL_PING` frame (`[id u128][timestamp u64]`).
    pub frame: Vec<u8>,
}

impl PingManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// Sends a ping (`sendPing`): the frame goes out, and the listener's
    /// slot is reserved for `attempts` attempts of `timeout_ms`.
    pub fn send(&mut self, id: u128, now_ms: u64, timeout_ms: u64, max_attempts: u32) -> Outgoing {
        self.in_flight.insert(
            id,
            Ping {
                timeout_ms,
                max_attempts,
                attempts: 1,
                started_ms: now_ms,
            },
        );
        Outgoing {
            id,
            frame: ping_frame(id, now_ms),
        }
    }

    /// The `CONTROL_PING` frame for a retry (`Ping.send` re-stamps).
    pub fn retry_frame(&self, id: u128, now_ms: u64) -> Option<Vec<u8>> {
        self.in_flight.get(&id)?;
        Some(ping_frame(id, now_ms))
    }

    /// A pong arrived: the matching ping's listener hears the round trip;
    /// an unmatched pong is dropped, as the `listeners.remove` miss does.
    pub fn on_pong(&mut self, id: u128, timestamp_ms: u64, now_ms: u64) -> Option<PingOutcome> {
        let ping = self.in_flight.remove(&id)?;
        Some(PingOutcome::Pong {
            attempts: ping.attempts,
            ping_ms: now_ms.saturating_sub(timestamp_ms),
        })
    }

    /// The sweep (`checkTimeouts`): every timed-out ping either retries
    /// (a new frame goes out, the attempt count climbs) or times out for
    /// good. `now_ms` is the clock; the frames to send come back in the
    /// outcomes' order.
    pub fn check_timeouts(&mut self, now_ms: u64) -> Vec<(u128, PingOutcome)> {
        let mut timed_out: Vec<u128> = self
            .in_flight
            .iter()
            .filter(|(_, p)| now_ms.saturating_sub(p.started_ms) >= p.timeout_ms)
            .map(|(&id, _)| id)
            .collect();
        timed_out.sort_unstable();
        let mut out = Vec::new();
        for id in timed_out {
            let attempts = match self.in_flight.get(&id) {
                Some(ping) => ping.attempts,
                None => continue,
            };
            if attempts >= self.in_flight[&id].max_attempts {
                self.in_flight.remove(&id);
                out.push((id, PingOutcome::Timeout { attempts }));
            } else {
                if let Some(ping) = self.in_flight.get_mut(&id) {
                    ping.attempts += 1;
                    ping.started_ms = now_ms;
                }
                out.push((id, PingOutcome::Retry { attempts }));
            }
        }
        out
    }

    /// How many pings are in flight (`listeners.isEmpty`'s inverse).
    pub fn pending(&self) -> usize {
        self.in_flight.len()
    }

    /// Resets everything (the world closed).
    pub fn reset(&mut self) {
        self.in_flight.clear();
    }
}

/// The `CONTROL_PING` frame: `[KIND_CONTROL][CONTROL_PING][id u128 LE]
/// [timestamp u64 LE]` (`PingPacket(id, timestamp)`).
pub fn ping_frame(id: u128, timestamp_ms: u64) -> Vec<u8> {
    let mut out = Vec::with_capacity(2 + 16 + 8);
    out.push(super::packets::KIND_CONTROL);
    out.push(CONTROL_PING);
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&timestamp_ms.to_le_bytes());
    out
}

/// The `CONTROL_PONG` frame, echoing both fields (`PingPacket` echoed).
pub fn pong_frame(id: u128, timestamp_ms: u64) -> Vec<u8> {
    let mut out = ping_frame(id, timestamp_ms);
    out[1] = CONTROL_PONG;
    out
}

/// Splits a ping or pong frame into its id and timestamp; `None` for
/// other frames and truncations.
pub fn parse_ping(frame: &[u8]) -> Option<(u8, u128, u64)> {
    if frame.len() < 2 + 16 + 8 {
        return None;
    }
    if frame[0] != super::packets::KIND_CONTROL {
        return None;
    }
    let kind = frame[1];
    if kind != CONTROL_PING && kind != CONTROL_PONG {
        return None;
    }
    let id = u128::from_le_bytes(frame[2..18].try_into().ok()?);
    let timestamp = u64::from_le_bytes(frame[18..26].try_into().ok()?);
    Some((kind, id, timestamp))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ping_sends_once_and_a_pong_completes_it() {
        let mut m = PingManager::new();
        let out = m.send(7, 0, 500, 10);
        assert_eq!(out.id, 7);
        assert_eq!(m.pending(), 1);
        // The frame carries the id and the timestamp.
        let (kind, id, ts) = parse_ping(&out.frame).unwrap();
        assert_eq!(kind, CONTROL_PING);
        assert_eq!(id, 7);
        assert_eq!(ts, 0);
        // The pong echoes and completes: 500 - 0 = 500 ms round trip.
        let pong = pong_frame(7, 0);
        assert_eq!(parse_ping(&pong).unwrap().0, CONTROL_PONG);
        assert_eq!(
            m.on_pong(7, 0, 500),
            Some(PingOutcome::Pong { attempts: 1, ping_ms: 500 })
        );
        assert_eq!(m.pending(), 0);
    }

    #[test]
    fn an_unmatched_pong_is_dropped() {
        let mut m = PingManager::new();
        assert_eq!(m.on_pong(99, 0, 500), None, "the remove miss");
    }

    #[test]
    fn a_timeout_retries_until_the_attempts_are_spent() {
        let mut m = PingManager::new();
        m.send(1, 0, 500, 3);
        // 500 ms: the first attempt times out and retries.
        let outcomes = m.check_timeouts(500);
        assert_eq!(outcomes, vec![(1, PingOutcome::Retry { attempts: 1 })]);
        let _ = m.retry_frame(1, 500).unwrap();
        // 1000 ms: the second attempt times out and retries.
        assert_eq!(
            m.check_timeouts(1000),
            vec![(1, PingOutcome::Retry { attempts: 2 })]
        );
        // 1500 ms: the third attempt was the last; it times out for good.
        assert_eq!(
            m.check_timeouts(1500),
            vec![(1, PingOutcome::Timeout { attempts: 3 })]
        );
        assert_eq!(m.pending(), 0);
    }

    #[test]
    fn a_retry_re_stamps_the_clock() {
        let mut m = PingManager::new();
        m.send(1, 0, 500, 3);
        let _ = m.check_timeouts(500);
        // The retry started at 500, so it survives until 1000.
        assert!(m.check_timeouts(999).is_empty());
        assert_eq!(m.check_timeouts(1000).len(), 1);
    }

    #[test]
    fn a_pong_arriving_between_sweeps_wins() {
        let mut m = PingManager::new();
        m.send(1, 0, 500, 3);
        // The pong lands just before the sweep would retry.
        assert!(m.check_timeouts(400).is_empty());
        assert!(m.on_pong(1, 0, 450).is_some());
        assert!(m.check_timeouts(1000).is_empty(), "nothing left to time out");
    }

    #[test]
    fn a_late_pong_after_a_timeout_is_dropped() {
        let mut m = PingManager::new();
        m.send(1, 0, 500, 1);
        assert_eq!(m.check_timeouts(500), vec![(1, PingOutcome::Timeout { attempts: 1 })]);
        assert_eq!(m.on_pong(1, 0, 600), None, "the slot is gone");
    }

    #[test]
    fn ping_and_pong_frames_round_trip() {
        let (kind, id, ts) = parse_ping(&ping_frame(0xDEAD_BEEF, 123_456)).unwrap();
        assert_eq!((kind, id, ts), (CONTROL_PING, 0xDEAD_BEEF, 123_456));
        let (kind, id, ts) = parse_ping(&pong_frame(1, 2)).unwrap();
        assert_eq!((kind, id, ts), (CONTROL_PONG, 1, 2));
        // Truncations and other kinds refuse.
        assert_eq!(parse_ping(&ping_frame(1, 2)[..10]), None);
        assert_eq!(parse_ping(&[]), None);
        let mut other = ping_frame(1, 2);
        other[1] = 99;
        assert_eq!(parse_ping(&other), None);
        let mut voice = ping_frame(1, 2);
        voice[0] = super::super::packets::KIND_VOICE;
        assert_eq!(parse_ping(&voice), None);
    }

    #[test]
    fn reset_forgets_everything() {
        let mut m = PingManager::new();
        m.send(1, 0, 500, 10);
        m.reset();
        assert_eq!(m.pending(), 0);
    }
}
