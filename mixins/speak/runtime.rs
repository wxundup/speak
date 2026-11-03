use std::cell::{Cell, RefCell};

/// The push-to-talk key's hold (the window's thread): down transmits,
/// up stops, and a press while a screen is open is ignored
/// (`MicThread.isTalking`'s input).
struct State {
    talking: bool,
}

thread_local! {
    static STATE: RefCell<State> = RefCell::new(State { talking: false });
    static WINDOW_TICK: Cell<u64> = const { Cell::new(0) };
}

pub fn key(playing: bool, pressed: bool, _get: impl Fn(&str) -> Option<String>) {
    if pressed && !playing {
        return;
    }
    STATE.with_borrow_mut(|s| s.talking = pressed);
}

/// Whether the local player holds the push-to-talk key.
pub fn talking() -> bool {
    STATE.with_borrow(|s| s.talking)
}

pub fn advance_window_tick() -> u64 {
    WINDOW_TICK.set(WINDOW_TICK.get() + 1);
    WINDOW_TICK.get()
}

/// The window's current tick, without advancing it: for timestamps.
pub fn window_tick() -> u64 {
    WINDOW_TICK.get()
}

/// Resets the sequence counter (a new world, a fresh session):
/// `MicThread`'s AtomicLong starts at 0.
pub fn reset_sequence() {
    super::mic::reset_sequence();
}

/// The world closed: the hold goes.
pub fn closed() {
    STATE.with_borrow_mut(|s| s.talking = false);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn none(_key: &str) -> Option<String> {
        None
    }

    #[test]
    fn the_key_holds_only_while_playing() {
        key(false, true, none);
        assert!(!talking(), "no key while a screen is open");
        key(true, true, none);
        assert!(talking());
        key(true, false, none);
        assert!(!talking());
        // A release while a screen is open still clears the hold.
        key(true, true, none);
        key(false, false, none);
        assert!(!talking());
    }

    #[test]
    fn the_tick_counter_advances_and_reads() {
        let before = window_tick();
        let advanced = advance_window_tick();
        assert_eq!(advanced, before + 1);
        assert_eq!(window_tick(), advanced);
    }

    #[test]
    fn the_world_closed_clears_the_hold() {
        key(true, true, none);
        closed();
        assert!(!talking());
    }
}
