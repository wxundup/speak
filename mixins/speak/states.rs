use std::collections::HashMap;

use super::packets::PlayerState;

#[derive(Default)]
pub struct StateManager {
    states: HashMap<u128, PlayerState>,
}

/// What a state change did, for the caller to broadcast.
#[derive(Clone, Debug, PartialEq)]
pub enum StateEvent {
    /// The state changed: broadcast it (`broadcastState`).
    Changed(PlayerState),
    /// The player's state went away: broadcast the removal
    /// (`broadcastRemoveState`).
    Removed(u128),
}

impl StateManager {
    pub fn new() -> Self {
        Self::default()
    }

    /// The player's state, if any (`getState`).
    pub fn get(&self, player: u128) -> Option<&PlayerState> {
        self.states.get(&player)
    }

    /// The player's state or a default disconnected one
    /// (`defaultDisconnectedState`): muted and disabled false,
    /// disconnected true — the shape a fresh login carries.
    pub fn get_or_default(&self, player: u128, name: &str) -> PlayerState {
        self.states.get(&player).cloned().unwrap_or(PlayerState {
            uuid: player,
            name: name.to_owned(),
            muted: false,
            force_muted: false,
            disabled: false,
            force_disabled: false,
            disconnected: true,
            group: None,
        })
    }

    /// A player logged in (`onPlayerLoggedIn`): a default disconnected
    /// state is installed and broadcast.
    pub fn on_login(&mut self, player: u128, name: &str) -> StateEvent {
        let state = PlayerState {
            uuid: player,
            name: name.to_owned(),
            muted: false,
            force_muted: false,
            disabled: false,
            force_disabled: false,
            disconnected: true,
            group: None,
        };
        self.states.insert(player, state.clone());
        StateEvent::Changed(state)
    }

    /// A player logged out (`onPlayerLoggedOut`): the state goes, and the
    /// removal broadcasts.
    pub fn on_logout(&mut self, player: u128) -> StateEvent {
        self.states.remove(&player);
        StateEvent::Removed(player)
    }

    /// The client told its own mute/disable (`updateStateChannel`): the
    /// flags land on the state (creating it if the voice session hasn't
    /// yet) and the state broadcasts. The group field survives the update.
    pub fn update_own_state(
        &mut self,
        player: u128,
        name: &str,
        muted: bool,
        disabled: bool,
    ) -> StateEvent {
        let mut state = self.get_or_default(player, name);
        state.muted = muted;
        state.disabled = disabled;
        state.disconnected = false;
        state.uuid = player;
        state.name = name.to_owned();
        self.states.insert(player, state.clone());
        StateEvent::Changed(state)
    }

    /// The server set a force flag (`evaluateForcedState`): forced states
    /// apply on top of the player's own. The client applies them locally
    /// too, so a revocation doesn't unmute unexpectedly
    /// (`applyForcedState`).
    pub fn set_forced(&mut self, player: u128, force_muted: bool, force_disabled: bool) -> StateEvent {
        let Some(state) = self.states.get_mut(&player) else {
            return StateEvent::Removed(player);
        };
        state.force_muted = force_muted;
        state.force_disabled = force_disabled;
        state.clone().into()
    }

    /// The group membership changed (the group manager calls through):
    /// the state's group is replaced (`setGroup`), one group at a time.
    pub fn set_group(&mut self, player: u128, group: Option<u128>) -> StateEvent {
        let Some(state) = self.states.get_mut(&player) else {
            return StateEvent::Removed(player);
        };
        state.group = group;
        state.clone().into()
    }

    /// The voice session came up (`onPlayerVoicechatConnect`):
    /// `disconnected` clears and the state broadcasts.
    pub fn set_connected(&mut self, player: u128) -> StateEvent {
        let Some(state) = self.states.get_mut(&player) else {
            return StateEvent::Removed(player);
        };
        state.disconnected = false;
        state.clone().into()
    }

    /// The voice session went down (`onPlayerVoicechatDisconnect`).
    pub fn set_disconnected(&mut self, player: u128) -> StateEvent {
        let Some(state) = self.states.get_mut(&player) else {
            return StateEvent::Removed(player);
        };
        state.disconnected = true;
        state.clone().into()
    }

    pub fn bulk_for(&self, player: u128) -> Vec<PlayerState> {
        self.states
            .values()
            .filter(|s| s.uuid != player)
            .cloned()
            .collect()
    }

    /// Whether the player would be muted or disabled with the force flags
    /// applied (`isMutedOrForceMuted` / `isDisabledOrForceDisabled`): the
    /// client's own mute plus a forced mute, the same for disabled.
    pub fn effective_muted(&self, player: u128) -> Option<bool> {
        self.states
            .get(&player)
            .map(|s| s.muted || s.force_muted)
    }

    pub fn effective_disabled(&self, player: u128) -> Option<bool> {
        self.states
            .get(&player)
            .map(|s| s.disabled || s.force_disabled)
    }

    /// Whether the player's voice session is down (`isDisconnected`).
    pub fn is_disconnected(&self, player: u128) -> bool {
        self.states
            .get(&player)
            .is_some_and(|s| s.disconnected)
    }

    /// Resets everything (the world closed).
    pub fn reset(&mut self) {
        self.states.clear();
    }
}

static STATES: std::sync::Mutex<Option<StateManager>> = std::sync::Mutex::new(None);

fn with_states<R>(f: impl FnOnce(&mut StateManager) -> R) -> R {
    let mut guard = STATES.lock().expect("the state table");
    guard.get_or_insert_with(StateManager::new);
    f(guard.as_mut().unwrap())
}

/// The player's effective mute (`isMutedOrForceMuted`), for the mix's
/// sender check. `None` when no state exists.
pub fn effective_muted(player: u128) -> Option<bool> {
    with_states(|s| s.effective_muted(player))
}

/// The player's effective disable (`isDisabledOrForceDisabled`), for the
/// mix's receiver and sender checks.
pub fn effective_disabled(player: u128) -> Option<bool> {
    with_states(|s| s.effective_disabled(player))
}

pub fn on_login(player: u128, name: &str) {
    with_states(|s| {
        let _ = s.on_login(player, name);
    });
}

pub fn set_group(player: u128, group: Option<u128>) {
    with_states(|s| {
        let _ = s.set_group(player, group);
    });
}

pub fn close(player: u128) {
    with_states(|s| {
        let _ = s.on_logout(player);
    });
}

pub fn reset() {
    with_states(|s| s.reset());
}

impl From<PlayerState> for StateEvent {
    fn from(state: PlayerState) -> Self {
        StateEvent::Changed(state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_login_installs_a_disconnected_state() {
        let mut m = StateManager::new();
        let event = m.on_login(1, "Alex");
        assert_eq!(
            event,
            StateEvent::Changed(PlayerState {
                uuid: 1,
                name: "Alex".into(),
                disconnected: true,
                ..PlayerState::default()
            })
        );
        assert!(m.is_disconnected(1));
    }

    #[test]
    fn the_client_state_update_fills_and_clears() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        let event = m.update_own_state(1, "Alex", true, false);
        let state = match event {
            StateEvent::Changed(s) => s,
            _ => panic!("changed"),
        };
        assert!(state.muted && !state.disabled);
        assert!(!state.disconnected, "a state update means the session is up");
        // The group survives an update (the listener only sets the two).
        m.set_group(1, Some(9));
        let state = match m.update_own_state(1, "Alex", false, false) {
            StateEvent::Changed(s) => s,
            _ => panic!("changed"),
        };
        assert_eq!(state.group, Some(9));
    }

    #[test]
    fn force_flags_apply_on_top_of_the_own_ones() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        m.update_own_state(1, "Alex", false, false);
        m.set_forced(1, true, false);
        assert_eq!(m.effective_muted(1), Some(true), "forced, though unmuted");
        assert_eq!(m.effective_disabled(1), Some(false));
        m.set_forced(1, false, false);
        assert_eq!(m.effective_muted(1), Some(false));
    }

    #[test]
    fn a_logout_removes_and_broadcasts_the_removal() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        assert_eq!(m.on_logout(1), StateEvent::Removed(1));
        assert_eq!(m.get(1), None);
        // A further update recreates the state, disconnected.
        let event = m.update_own_state(1, "Alex", true, false);
        assert!(matches!(event, StateEvent::Changed(s) if s.disconnected == false));
    }

    #[test]
    fn the_bulk_skips_the_recipient_and_the_group_flows() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        m.on_login(2, "Sam");
        m.on_login(3, "Rex");
        m.set_group(2, Some(9));
        let bulk = m.bulk_for(1);
        assert_eq!(bulk.len(), 2, "the recipient's own state stays out");
        let sam = bulk.iter().find(|s| s.uuid == 2).unwrap();
        assert_eq!(sam.group, Some(9));
    }

    #[test]
    fn the_connection_toggles_the_disconnected_flag() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        m.set_connected(1);
        assert!(!m.is_disconnected(1));
        m.set_disconnected(1);
        assert!(m.is_disconnected(1));
    }

    #[test]
    fn reset_forgets_everything() {
        let mut m = StateManager::new();
        m.on_login(1, "Alex");
        m.reset();
        assert_eq!(m.get(1), None);
    }
}
