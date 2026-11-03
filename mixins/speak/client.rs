
use std::cell::Cell;

thread_local! {
    /// The whisper key's hold, the window's thread (like the runtime's
    /// talking flag): down whispers, up stops, never persisted.
    static WHISPERING: Cell<bool> = const { Cell::new(false) };
}

/// Whether a stored flag reads "true" (the settings' boolean convention:
/// an exact match; missing, "false" or anything else means false).
fn flag(get: impl Fn(&str) -> Option<String>, key: &str) -> bool {
    get(key).as_deref() == Some("true")
}

/// The settings key a player's volume lives under
/// (`player-volumes.properties` keys by UUID string).
fn player_key(uuid: u128) -> String {
    format!("player_{uuid}")
}

/// What a key event resolved to (`KeyEvents`' actions for the mute,
/// whisper, hide-icons and disable keys).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    None,
    /// The mute key toggled; the new muted state.
    MuteToggled(bool),
    /// Whispering started or stopped.
    Whispering(bool),
    /// The hide-icons key toggled; the new hidden state.
    HideToggled(bool),
    /// The disable key toggled; the new disabled state.
    DisableToggled(bool),
}

pub fn on_key(
    mute_bound: &str,
    whisper_bound: &str,
    key: &str,
    pressed: bool,
    playing: bool,
    get: impl Fn(&str) -> Option<String>,
    set: impl Fn(&str, &str),
) -> Action {
    if key == mute_bound {
        if !pressed || !playing {
            return Action::None;
        }
        let muted = !muted(get);
        set("muted", if muted { "true" } else { "false" });
        return Action::MuteToggled(muted);
    }
    if key == whisper_bound {
        if pressed && !playing {
            return Action::None;
        }
        WHISPERING.with(|w| w.set(pressed));
        return Action::Whispering(pressed);
    }
    Action::None
}

/// Whether the local player muted their microphone (persisted "muted",
/// default `ClientConfig.muted` = true for VA users; the caller's first
/// migration decides).
pub fn muted(get: impl Fn(&str) -> Option<String>) -> bool {
    flag(get, "muted")
}

/// Whether the local player disabled the whole voice chat (persisted
/// "disabled", default false). Disabled means: never transmit, and the
/// HUD shows the disabled tag.
pub fn disabled(get: impl Fn(&str) -> Option<String>) -> bool {
    flag(get, "disabled")
}

/// Whether the local player hears nobody (persisted "deafened", default
/// false): the receiver drops what the mix sends, and the HUD shows the
/// struck-through speaker.
pub fn deafened(get: impl Fn(&str) -> Option<String>) -> bool {
    flag(get, "deafened")
}

/// Set the disabled state (the menu's row, the disable key, the settings
/// page's button), persisted by the caller.
pub fn set_disabled(disabled: bool, set: impl Fn(&str, &str)) {
    set("disabled", if disabled { "true" } else { "false" });
}

/// Set the deafened state (the menu's row), persisted by the caller.
pub fn set_deafened(deafened: bool, set: impl Fn(&str, &str)) {
    set("deafened", if deafened { "true" } else { "false" });
}

/// Whether the whisper key is currently held.
pub fn whispering() -> bool {
    WHISPERING.with(Cell::get)
}

pub fn transmitting(
    activation_is_voice: bool,
    push_to_talk_down: bool,
    get: impl Fn(&str) -> Option<String>,
) -> bool {
    if muted(&get) || disabled(&get) {
        return false;
    }
    if activation_is_voice {
        // VOX: the mic level decides, and there is no mic yet (the `get`
        // the caller passes would read the sensitivity; nothing to ask).
        false
    } else {
        push_to_talk_down
    }
}

/// The volume a receiver set for a specific player, 0.0..=2.0, default
/// 1.0 (`PLAYER_VOLUME_CONFIG.getVolume`; the slider's range is [0, 4]
/// but the voice path clamps to [0, 2]).
pub fn player_volume(uuid: u128, get: impl Fn(&str) -> Option<String>) -> f32 {
    get(&player_key(uuid))
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(1.0)
        .clamp(0.0, 2.0)
}

/// Nudge a player's volume by `delta` (e.g. one wheel notch), clamped
/// 0.0..=2.0; writes it through `set` and returns the new value.
pub fn adjust_player_volume(
    uuid: u128,
    delta: f32,
    get: impl Fn(&str) -> Option<String>,
    set: impl Fn(&str, &str),
) -> f32 {
    let volume = (player_volume(uuid, &get) + delta).clamp(0.0, 2.0);
    set(&player_key(uuid), &volume.to_string());
    volume
}

/// The world closed: the whisper hold goes.
pub fn closed() {
    WHISPERING.with(|w| w.set(false));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> (impl Fn(&str) -> Option<String>, impl Fn(&str, &str)) {
        let shared: &'static std::cell::RefCell<Vec<(String, String)>> =
            Box::leak(Box::new(std::cell::RefCell::new(Vec::new())));
        let get = move |k: &str| {
            shared
                .borrow()
                .iter()
                .find(|(key, _)| key == k)
                .map(|(_, v)| v.clone())
        };
        let set = move |k: &str, v: &str| {
            let mut files = shared.borrow_mut();
            files.retain(|(key, _)| key != k);
            files.push((k.to_owned(), v.to_owned()));
        };
        (get, set)
    }

    #[test]
    fn the_mute_key_toggles_the_persisted_flag() {
        let (get, set) = store();
        assert!(!muted(&get), "the tests start unmuted");
        assert_eq!(
            on_key("key.keyboard.m", "", "key.keyboard.m", true, true, &get, &set),
            Action::MuteToggled(true)
        );
        assert!(muted(&get));
        assert_eq!(
            on_key("key.keyboard.m", "", "key.keyboard.m", true, true, &get, &set),
            Action::MuteToggled(false)
        );
        assert!(!muted(&get));
    }

    #[test]
    fn a_mute_press_needs_playing() {
        let (get, set) = store();
        assert_eq!(
            on_key("key.keyboard.m", "", "key.keyboard.m", true, false, &get, &set),
            Action::None
        );
        assert!(!muted(&get));
    }

    #[test]
    fn the_whisper_key_is_held_not_persisted() {
        let (get, set) = store();
        assert_eq!(
            on_key("", "key.keyboard.b", "key.keyboard.b", true, true, &get, &set),
            Action::Whispering(true)
        );
        assert!(whispering());
        assert_eq!(
            on_key("", "key.keyboard.b", "key.keyboard.b", false, true, &get, &set),
            Action::Whispering(false)
        );
        assert!(!whispering());
        // Nothing was written.
        assert_eq!(get("whisper"), None);
    }

    #[test]
    fn a_whisper_release_always_stops_a_held_whisper() {
        let (get, set) = store();
        on_key("", "key.keyboard.b", "key.keyboard.b", true, true, &get, &set);
        // The key releases while a screen is open (playing false): the
        // whisper still clears, as the runtime's key does.
        assert_eq!(
            on_key("", "key.keyboard.b", "key.keyboard.b", false, false, &get, &set),
            Action::Whispering(false)
        );
        assert!(!whispering());
    }

    #[test]
    fn a_whisper_press_while_not_playing_is_ignored() {
        let (get, set) = store();
        assert_eq!(
            on_key("", "key.keyboard.b", "key.keyboard.b", true, false, &get, &set),
            Action::None
        );
        assert!(!whispering());
    }

    #[test]
    fn keys_bound_elsewhere_are_none() {
        let (get, set) = store();
        assert_eq!(
            on_key("key.keyboard.m", "", "key.keyboard.n", true, true, &get, &set),
            Action::None
        );
    }

    #[test]
    fn muted_and_disabled_default_false_and_round_trip() {
        let (get, set) = store();
        assert!(!muted(&get));
        assert!(!disabled(&get));
        assert!(!deafened(&get));
        set_disabled(true, &set);
        assert!(disabled(&get));
        set_disabled(false, &set);
        assert!(!disabled(&get));
        set_deafened(true, &set);
        assert!(deafened(&get));
        set_deafened(false, &set);
        assert!(!deafened(&get));
    }

    #[test]
    fn transmitting_needs_the_key_and_no_mute_and_no_disable() {
        let (get, set) = store();
        // Push to talk, key down: transmit.
        assert!(transmitting(false, true, &get));
        assert!(!transmitting(false, false, &get));
        // Muted: never.
        set("muted", "true");
        assert!(!transmitting(false, true, &get));
        set("muted", "false");
        set("disabled", "true");
        assert!(!transmitting(false, true, &get));
        set("disabled", "false");
        // Voice activation: the mic decides, and there is no mic yet.
        assert!(!transmitting(true, true, &get));
    }

    #[test]
    fn player_volumes_default_to_one_and_clamp() {
        let (get, set) = store();
        assert_eq!(player_volume(7, &get), 1.0);
        assert_eq!(
            adjust_player_volume(7, 0.5, &get, &set),
            1.5,
            "a wheel notch"
        );
        assert_eq!(player_volume(7, &get), 1.5);
        // The clamps: 0 and 2.
        assert_eq!(adjust_player_volume(7, -5.0, &get, &set), 0.0);
        assert_eq!(adjust_player_volume(7, 5.0, &get, &set), 2.0);
        // A malformed stored value falls back to the default.
        set("player_8", "nonsense");
        assert_eq!(player_volume(8, &get), 1.0);
        // Past the slider's 4.0 the voice path still clamps to 2.
        set("player_9", "4.0");
        assert_eq!(player_volume(9, &get), 2.0);
    }

    #[test]
    fn the_whisper_hold_resets_with_the_world() {
        let (get, set) = store();
        on_key("", "key.keyboard.b", "key.keyboard.b", true, true, &get, &set);
        assert!(whispering());
        closed();
        assert!(!whispering());
    }
}
