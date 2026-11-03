use std::collections::HashMap;

use super::activation::GroupType;
use super::frame::{MAX_GROUP_NAME_LENGTH, MAX_GROUP_PASSWORD_LENGTH};
use super::packets::Group;

pub mod replies {
    pub const JOINED: &str = "Speak.: joined the group";
    pub const CREATED: &str = "Speak.: created the group";
    pub const ALREADY_IN: &str = "Speak.: already in that group";
    pub const WRONG_PASSWORD: &str = "Speak.: wrong password";
    pub const NOT_IN_GROUP: &str = "Speak.: you are not in a group";
    pub const LEFT: &str = "Speak.: left the group";
    pub const GROUPS_DISABLED: &str = "Speak.: groups are disabled on this server";
    pub const NO_PERMISSION: &str = "Speak.: you may not use groups here";
    pub const BAD_NAME: &str = "Speak.: that group name is not allowed";
    pub const UNKNOWN_GROUP: &str = "Speak.: no such group";
    pub const INVITED: &str = "Speak.: invited";
    pub const NOT_IN_GROUP_INVITE: &str = "Speak.: join a group before inviting";
    pub const NO_SUCH_PLAYER: &str = "Speak.: no such player online";
}

/// The groups by id (`ServerGroupManager.groups`), plus the state table's
/// membership side (`PlayerStateManager.states`'s group field, held here as
/// the authoritative `member_of` map so the routing rules are O(1)).
#[derive(Default)]
pub struct GroupManager {
    groups: HashMap<u128, GroupEntry>,
    /// Each member's group id, held alongside the states so the routing
    /// rules are O(1).
    member_of: HashMap<u128, u128>,
    /// Whether the server allows groups (`SERVER_CONFIG.groupsEnabled`).
    pub groups_enabled: bool,
}

/// The server's own group: what the clients' `Group` frame carries, plus
/// the password itself, which never leaves the server (`Group.password`).
#[derive(Clone, Debug)]
pub struct GroupEntry {
    pub id: u128,
    pub name: String,
    pub password: Option<String>,
    pub persistent: bool,
    pub hidden: bool,
    pub group_type: GroupType,
}

impl GroupEntry {
    /// The client-facing frame (`toClientGroup`): a has-password flag,
    /// never the password itself.
    pub fn to_client_group(&self) -> Group {
        Group {
            id: self.id,
            name: self.name.clone(),
            has_password: self.password.is_some(),
            persistent: self.persistent,
            hidden: self.hidden,
            group_type: match self.group_type {
                GroupType::Normal => 0,
                GroupType::Open => 1,
                GroupType::Isolated => 2,
            },
        }
    }

    /// `Group.isOpen` / `isNormal` / `isIsolated` as predicates.
    pub fn is_open(&self) -> bool {
        self.group_type == GroupType::Open
    }

    pub fn is_isolated(&self) -> bool {
        self.group_type == GroupType::Isolated
    }
}

/// What a group operation did, for the caller to send where it belongs.
#[derive(Clone, Debug, PartialEq)]
pub enum GroupOutcome {
    /// The line for the acting player.
    Replied(&'static str),
    /// The group's name and the line for the acting player (joined or
    /// created): the caller also tells the client its group changed.
    Joined(String, &'static str),
    /// The player left their group's name (or had none).
    Left(Option<String>),
    /// The group a player was invited to, and the invited player's uuid.
    Invited(String, u128),
}

impl GroupManager {
    pub fn new(groups_enabled: bool) -> Self {
        Self {
            groups: HashMap::new(),
            member_of: HashMap::new(),
            groups_enabled,
        }
    }

    /// The group by id (`getGroup`).
    pub fn get(&self, id: u128) -> Option<&GroupEntry> {
        self.groups.get(&id)
    }

    pub fn named(&self, name: &str) -> Option<&GroupEntry> {
        self.groups
            .values()
            .find(|g| g.name.eq_ignore_ascii_case(name))
    }

    /// The player's group (`getPlayerGroup`).
    pub fn player_group(&self, player: u128) -> Option<&GroupEntry> {
        let id = self.member_of.get(&player)?;
        self.groups.get(id)
    }

    /// The group's name the player is in, for the menu's header.
    pub fn player_group_name(&self, player: u128) -> Option<String> {
        self.player_group(player).map(|g| g.name.clone())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        &mut self,
        id: u128,
        name: &str,
        password: Option<&str>,
        persistent: bool,
        hidden: bool,
        group_type: GroupType,
        creator: u128,
    ) -> GroupOutcome {
        if !self.groups_enabled {
            return GroupOutcome::Replied(replies::GROUPS_DISABLED);
        }
        if !group_name_ok(name) {
            return GroupOutcome::Replied(replies::BAD_NAME);
        }
        if password.is_some_and(|p| !group_password_ok(p)) {
            return GroupOutcome::Replied(replies::BAD_NAME);
        }
        let entry = GroupEntry {
            id,
            name: name.to_owned(),
            password: password.map(str::to_owned),
            persistent,
            hidden,
            group_type,
        };
        self.groups.insert(id, entry);
        self.member_of.insert(creator, id);
        GroupOutcome::Joined(name.to_owned(), replies::CREATED)
    }

    pub fn join_by_id(&mut self, id: u128, player: u128, password: Option<&str>) -> GroupOutcome {
        if !self.groups_enabled {
            return GroupOutcome::Replied(replies::GROUPS_DISABLED);
        }
        let Some(group) = self.groups.get(&id) else {
            return GroupOutcome::Replied(replies::UNKNOWN_GROUP);
        };
        let password_ok = group.password.is_none() || group.password.as_deref() == password;
        let group_name = password_ok.then(|| group.name.clone());
        let Some(group_name) = group_name else {
            return GroupOutcome::Replied(replies::WRONG_PASSWORD);
        };
        if self.member_of.get(&player) == Some(&id) {
            return GroupOutcome::Replied(replies::ALREADY_IN);
        }
        // One group at a time: the old membership goes first, and the group
        // it emptied is cleaned up (the create flow's own join included).
        if let Some(old) = self.member_of.insert(player, id) {
            if old != id {
                self.cleanup();
            }
        }
        GroupOutcome::Joined(group_name, replies::JOINED)
    }

    /// Joins by name (`joinGroupByName` in the commands): same rules,
    /// resolved case-insensitively.
    pub fn join_by_name(&mut self, name: &str, player: u128, password: Option<&str>) -> GroupOutcome {
        let id = self.named(name).map(|g| g.id);
        match id {
            Some(id) => self.join_by_id(id, player, password),
            None if self.groups_enabled => GroupOutcome::Replied(replies::UNKNOWN_GROUP),
            None => GroupOutcome::Replied(replies::GROUPS_DISABLED),
        }
    }

    /// Leaves the current group (`leaveGroup`): the state clears, and the
    /// empty groups go (`cleanupGroups` runs after every leave and logout).
    pub fn leave(&mut self, player: u128) -> GroupOutcome {
        let Some(id) = self.member_of.remove(&player) else {
            return GroupOutcome::Replied(replies::NOT_IN_GROUP);
        };
        let name = self.groups.get(&id).map(|g| g.name.clone());
        self.cleanup();
        GroupOutcome::Left(name)
    }

    pub fn invite(&mut self, inviter: u128, target: u128, target_online: bool) -> GroupOutcome {
        if !self.groups_enabled {
            return GroupOutcome::Replied(replies::GROUPS_DISABLED);
        }
        if !target_online {
            return GroupOutcome::Replied(replies::NO_SUCH_PLAYER);
        }
        let Some(group) = self.player_group(inviter) else {
            return GroupOutcome::Replied(replies::NOT_IN_GROUP_INVITE);
        };
        GroupOutcome::Invited(group.name.clone(), target)
    }

    /// A player left the server (`onPlayerLoggedOut` → cleanupGroups).
    pub fn on_disconnect(&mut self, player: u128) {
        let _ = self.member_of.remove(&player);
        self.cleanup();
    }

    /// Removes the non-persistent groups nobody belongs to
    /// (`cleanupGroups`: the used ids come from the states, the rest go —
    /// persistent ones stay).
    pub fn cleanup(&mut self) {
        let used: std::collections::HashSet<u128> = self.member_of.values().copied().collect();
        let gone: Vec<u128> = self
            .groups
            .values()
            .filter(|g| !g.persistent && !used.contains(&g.id))
            .map(|g| g.id)
            .collect();
        for id in gone {
            self.groups.remove(&id);
        }
    }

    pub fn listable(&self) -> Vec<Group> {
        let mut groups: Vec<Group> = self
            .groups
            .values()
            .filter(|g| !g.hidden)
            .map(|g| g.to_client_group())
            .collect();
        groups.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        groups
    }

    /// Whether the sender's group is isolated (`Group.isIsolated`), for
    /// the mix's rules.
    pub fn sender_isolated(&self, sender: u128) -> bool {
        self.player_group(sender).is_some_and(|g| g.is_isolated())
    }

    /// Whether sender and receiver share a group (the mix's same-group
    /// rule).
    pub fn same_group(&self, a: u128, b: u128) -> bool {
        match (self.member_of.get(&a), self.member_of.get(&b)) {
            (Some(x), Some(y)) => x == y,
            _ => false,
        }
    }

    /// Whether the receiver's group is isolated (`Group.isIsolated` on the
    /// receiver's side): an isolated group's members hear no proximity.
    pub fn receiver_isolated(&self, receiver: u128) -> bool {
        self.player_group(receiver).is_some_and(|g| g.is_isolated())
    }

    /// Whether the sender's group is open (`Group.isOpen`): an open
    /// group's sender also bleeds to proximity.
    pub fn sender_open(&self, sender: u128) -> bool {
        self.player_group(sender).is_some_and(|g| g.is_open())
    }

    /// Every listable group's member count (`getGroups` joined with the
    /// state table's membership side), for the group list.
    pub fn members_of(&self, id: u128) -> usize {
        self.member_of.values().filter(|&&g| g == id).count()
    }

    pub fn reset(&mut self) {
        self.groups.clear();
        self.member_of.clear();
    }
}

static GROUPS: std::sync::Mutex<Option<GroupManager>> = std::sync::Mutex::new(None);

pub fn with_groups<R>(f: impl FnOnce(&mut GroupManager) -> R) -> R {
    let mut guard = GROUPS.lock().expect("the group table");
    guard.get_or_insert_with(|| GroupManager::new(super::config::SERVER_GROUPS_ENABLED));
    f(guard.as_mut().unwrap())
}

pub fn player_group_name(player: u128) -> Option<String> {
    with_groups(|g| g.player_group_name(player).map(|name| name.clone()))
}

pub fn same_group(a: u128, b: u128) -> bool {
    with_groups(|g| g.same_group(a, b))
}

pub fn sender_open(player: u128) -> bool {
    with_groups(|g| g.sender_open(player))
}

pub fn sender_isolated(player: u128) -> bool {
    with_groups(|g| g.sender_isolated(player))
}

pub fn receiver_isolated(player: u128) -> bool {
    with_groups(|g| g.receiver_isolated(player))
}

pub fn leave(player: u128) {
    with_groups(|g| {
        let _ = g.leave(player);
    });
}

pub fn on_disconnect(player: u128) {
    with_groups(|g| g.on_disconnect(player));
}

pub fn reset() {
    with_groups(|g| g.reset());
}

/// The name rules (`Voicechat.GROUP_REGEX`): 1..=24 characters, no control
/// characters, and no whitespace or control character first.
pub fn group_name_ok(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if first.is_control() || first.is_whitespace() {
        return false;
    }
    name.chars().count() <= MAX_GROUP_NAME_LENGTH && name.chars().all(|c| !c.is_control())
}

pub fn group_password_ok(password: &str) -> bool {
    password.chars().count() <= MAX_GROUP_PASSWORD_LENGTH
        && password.chars().all(|c| !c.is_control())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manager() -> GroupManager {
        GroupManager::new(true)
    }

    #[test]
    fn create_joins_the_creator_and_the_frame_hides_the_password() {
        let mut m = manager();
        let outcome = m.create(1, "team", Some("pw"), false, false, GroupType::Normal, 10);
        assert_eq!(outcome, GroupOutcome::Joined("team".into(), replies::CREATED));
        assert_eq!(m.player_group_name(10).as_deref(), Some("team"));
        let frame = m.get(1).unwrap().to_client_group();
        assert!(frame.has_password);
        assert_eq!(frame.group_type, 0);
        // The name check runs on create.
        assert_eq!(
            m.create(2, "", None, false, false, GroupType::Normal, 10),
            GroupOutcome::Replied(replies::BAD_NAME)
        );
    }

    #[test]
    fn joining_needs_the_right_password() {
        let mut m = manager();
        m.create(1, "team", Some("pw"), false, false, GroupType::Normal, 10);
        assert_eq!(
            m.join_by_id(1, 20, None),
            GroupOutcome::Replied(replies::WRONG_PASSWORD),
            "a missing password refuses too"
        );
        assert_eq!(
            m.join_by_id(1, 20, Some("nope")),
            GroupOutcome::Replied(replies::WRONG_PASSWORD)
        );
        assert_eq!(
            m.join_by_id(1, 20, Some("pw")),
            GroupOutcome::Joined("team".into(), replies::JOINED)
        );
        assert_eq!(
            m.join_by_id(1, 20, Some("pw")),
            GroupOutcome::Replied(replies::ALREADY_IN)
        );
    }

    #[test]
    fn joining_by_name_matches_case_insensitively() {
        let mut m = manager();
        m.create(1, "Team", None, false, false, GroupType::Normal, 10);
        assert_eq!(
            m.join_by_name("TEAM", 20, None),
            GroupOutcome::Joined("Team".into(), replies::JOINED)
        );
        assert_eq!(m.join_by_name("nope", 20, None), GroupOutcome::Replied(replies::UNKNOWN_GROUP));
    }

    #[test]
    fn leave_clears_the_state_and_cleans_empty_groups() {
        let mut m = manager();
        m.create(1, "team", None, false, false, GroupType::Normal, 10);
        m.join_by_id(1, 20, None);
        assert_eq!(m.leave(10), GroupOutcome::Left(Some("team".into())));
        // 20 still holds the group.
        assert!(m.get(1).is_some());
        assert_eq!(m.leave(20), GroupOutcome::Left(Some("team".into())));
        // Nobody holds it: the non-persistent group went.
        assert!(m.get(1).is_none());
        assert_eq!(m.leave(20), GroupOutcome::Replied(replies::NOT_IN_GROUP));
    }

    #[test]
    fn persistent_groups_survive_an_empty_membership() {
        let mut m = manager();
        m.create(1, "club", None, true, false, GroupType::Normal, 10);
        m.leave(10);
        assert!(m.get(1).is_some(), "a persistent group stays");
    }

    #[test]
    fn a_switch_joins_the_new_group_and_leaves_the_old() {
        let mut m = manager();
        m.create(1, "a", None, false, false, GroupType::Normal, 10);
        m.create(2, "b", None, false, false, GroupType::Normal, 10);
        m.join_by_id(1, 20, None);
        m.join_by_id(2, 20, None);
        // Joining b removed 20 from a; a went empty and was cleaned.
        assert!(m.get(1).is_none(), "the emptied group was cleaned up");
        assert!(m.get(2).is_some());
        assert_eq!(m.player_group_name(20).as_deref(), Some("b"));
    }
    #[test]
    fn invites_need_a_group_and_an_online_target() {
        let mut m = manager();
        m.create(1, "team", None, false, false, GroupType::Normal, 10);
        assert_eq!(
            m.invite(30, 40, true),
            GroupOutcome::Replied(replies::NOT_IN_GROUP_INVITE)
        );
        assert_eq!(
            m.invite(10, 40, false),
            GroupOutcome::Replied(replies::NO_SUCH_PLAYER)
        );
        assert_eq!(m.invite(10, 40, true), GroupOutcome::Invited("team".into(), 40));
    }

    #[test]
    fn disabled_groups_refuse_everything() {
        let mut m = GroupManager::new(false);
        assert_eq!(
            m.create(1, "team", None, false, false, GroupType::Normal, 10),
            GroupOutcome::Replied(replies::GROUPS_DISABLED)
        );
        assert_eq!(
            m.join_by_name("team", 20, None),
            GroupOutcome::Replied(replies::GROUPS_DISABLED)
        );
        assert_eq!(
            m.invite(10, 20, true),
            GroupOutcome::Replied(replies::GROUPS_DISABLED)
        );
    }

    #[test]
    fn the_routing_predicates_drive_the_mix() {
        let mut m = manager();
        m.create(1, "open", None, false, false, GroupType::Open, 10);
        m.create(3, "normal", None, false, false, GroupType::Normal, 11);
        m.create(2, "iso", None, false, false, GroupType::Isolated, 20);
        m.join_by_id(1, 12, None);
        m.join_by_id(2, 21, None);
        assert!(m.same_group(10, 12));
        assert!(!m.same_group(10, 21));
        assert!(m.sender_open(10), "an open group bleeds");
        assert!(m.sender_isolated(20) && m.receiver_isolated(21));
        // A NORMAL group neither bleeds nor isolates: its members hear
        // each other, and its sender's voice also stops at proximity.
        assert!(!m.sender_open(11), "members of a normal group do not bleed");
        assert!(!m.sender_isolated(11) && !m.receiver_isolated(11));
    }

    #[test]
    fn the_listable_groups_skip_hidden_ones_and_sort_by_name() {
        let mut m = manager();
        m.create(3, "charlie", None, false, false, GroupType::Normal, 10);
        m.create(1, "alpha", None, false, true, GroupType::Normal, 10);
        m.create(2, "bravo", None, false, false, GroupType::Normal, 10);
        let list = m.listable();
        let names: Vec<&str> = list.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["bravo", "charlie"], "hidden groups stay off the list");
    }

    #[test]
    fn the_name_and_password_rules_follow_the_regex() {
        assert!(group_name_ok("team"));
        assert!(group_name_ok(&"x".repeat(MAX_GROUP_NAME_LENGTH)));
        assert!(!group_name_ok(&"x".repeat(MAX_GROUP_NAME_LENGTH + 1)));
        assert!(!group_name_ok(""));
        assert!(!group_name_ok(" team"));
        assert!(!group_name_ok("te\u{0}am"));
        assert!(group_password_ok("pass word"));
        assert!(!group_password_ok(&"p".repeat(MAX_GROUP_PASSWORD_LENGTH + 1)));
        assert!(!group_password_ok("pa\u{1}ss"));
    }

    #[test]
    fn reset_forgets_everything() {
        let mut m = manager();
        m.create(1, "team", None, true, false, GroupType::Normal, 10);
        m.reset();
        assert!(m.get(1).is_none());
        assert_eq!(m.player_group_name(10), None);
    }
}
