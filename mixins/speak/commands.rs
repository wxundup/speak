
use super::activation::GroupType;
use super::groups::{GroupManager, GroupOutcome, replies};
use super::packets::NativeCommand;

#[allow(unused_imports)]

/// The commands the mod registers (`VoicechatCommands.register`), as
/// usages the server's command tree merges.
pub fn commands() -> Vec<NativeCommand> {
    vec![
        NativeCommand::new("speak"),
        NativeCommand::new("speak help"),
        NativeCommand::new("speak invite <player:player>"),
        NativeCommand::new("speak join <name:word>"),
        NativeCommand::new("speak join <name:word> <password:word>"),
        NativeCommand::new("speak leave"),
        NativeCommand::new("speak group"),
        NativeCommand::new("speak create <name:word>"),
        NativeCommand::new("speak create <name:word> <password:word>"),
        NativeCommand::new("speak create <name:word> <password:word> <type:word>"),
    ]
}

/// What a command answered, for the caller to send and broadcast.
#[derive(Debug, PartialEq)]
pub enum CommandReply {
    /// A line for the acting player only.
    Tell(String),
    /// A line for the acting player, plus the group's state changed: the
    /// caller sends the group's name to the client and broadcasts the
    /// group list.
    GroupChanged { tell: String, group: Option<String> },
    /// Not the mod's command (the caller leaves `ran` false).
    NotMine,
}

/// Runs a player's command (`VoicechatCommands`' executors): `words` is
/// the line without the leading command name. `roster` is the online
/// players (uuid, name), case-insensitive for invites.
pub fn perform(
    manager: &mut GroupManager,
    player: u128,
    player_name: &str,
    words: &[&str],
    roster: &[(u128, String)],
) -> CommandReply {
    match words.first().copied().unwrap_or("help") {
        "help" => CommandReply::Tell(help_line()),
        "group" => CommandReply::Tell(group_list(manager)),
        "leave" => {
            let outcome = manager.leave(player);
            finish_group_outcome(manager, player, player_name, outcome, None)
        }
        "join" => {
            let Some(name) = words.get(1).copied() else {
                return CommandReply::Tell("Speak.: /speak join <name> [password]".into());
            };
            let password = words.get(2).copied();
            let outcome = manager.join_by_name(name, player, password);
            finish_group_outcome(manager, player, player_name, outcome, None)
        }
        "create" => {
            let Some(name) = words.get(1).copied() else {
                return CommandReply::Tell("Speak.: /speak create <name> [password] [type]".into());
            };
            let password = words.get(2).copied();
            let group_type = match words.get(3).copied().map(str::to_ascii_lowercase) {
                None => GroupType::Normal,
                Some(t) if t == "open" => GroupType::Open,
                Some(t) if t == "isolated" => GroupType::Isolated,
                Some(_) => {
                    return CommandReply::Tell(
                        "Speak.: the type is 'open' or 'isolated' (default: normal)".into(),
                    );
                }
            };
            let id = group_id(player, name);
            let outcome =
                manager.create(id, name, password, false, false, group_type, player);
            finish_group_outcome(manager, player, player_name, outcome, None)
        }
        "invite" => {
            let Some(target_name) = words.get(1).copied() else {
                return CommandReply::Tell("Speak.: /speak invite <player>".into());
            };
            let Some(target) = roster
                .iter()
                .find(|(_, name)| name.eq_ignore_ascii_case(target_name))
                .map(|(uuid, _)| *uuid)
            else {
                return CommandReply::Tell(replies::NO_SUCH_PLAYER.to_owned());
            };
            let target_online = roster.iter().any(|(uuid, _)| *uuid == target);
            match manager.invite(player, target, target_online) {
                GroupOutcome::Invited(group, _) => CommandReply::Tell(format!(
                    "{} {target_name} to '{}' - they join with /speak join {}",
                    replies::INVITED, group, group
                )),
                GroupOutcome::Replied(line) => CommandReply::Tell(line.to_owned()),
                _ => CommandReply::Tell(replies::NOT_IN_GROUP.to_owned()),
            }
        }
        _ => CommandReply::Tell(help_line()),
    }
}

fn finish_group_outcome(
    _manager: &mut GroupManager,
    _player: u128,
    _player_name: &str,
    outcome: GroupOutcome,
    _extra: Option<()>,
) -> CommandReply {
    match outcome {
        GroupOutcome::Joined(name, line) => {
            CommandReply::GroupChanged { tell: format!("{line} '{name}'"), group: Some(name) }
        }
        GroupOutcome::Left(Some(name)) => CommandReply::GroupChanged {
            tell: format!("{} '{name}'", replies::LEFT),
            group: None,
        },
        GroupOutcome::Left(None) => CommandReply::Tell(replies::NOT_IN_GROUP.to_owned()),
        GroupOutcome::Replied(line) => CommandReply::Tell(line.to_owned()),
        GroupOutcome::Invited(_, _) => CommandReply::Tell(replies::NOT_IN_GROUP.to_owned()),
    }
}

/// The help line (`VoicechatCommands.help`'s key list, in our words).
fn help_line() -> String {
    "Speak.: /speak <help|group|invite|join|leave|create>".into()
}

/// The group list (`/voicechat group`'s summary): each listable group's
/// name, type, password flag and member count.
fn group_list(manager: &GroupManager) -> String {
    let groups = manager.listable();
    if groups.is_empty() {
        return "Speak.: no groups".into();
    }
    let mut out = String::from("Speak. groups:");
    for group in groups {
        let kind = match group.group_type {
            0 => "",
            1 => " open",
            _ => " isolated",
        };
        let lock = if group.has_password { " [password]" } else { "" };
        let members = manager.members_of(group.id);
        out.push_str(&format!(
            "\n  {}{}{} - {} member{}",
            group.name,
            kind,
            lock,
            members,
            if members == 1 { "" } else { "s" }
        ));
    }
    out
}

fn group_id(creator: u128, name: &str) -> u128 {
    let mut hash: u128 = 0xcbf2_9ce4_8422_2325;
    for byte in creator.to_le_bytes().iter().chain(name.as_bytes()) {
        hash ^= u128::from(*byte);
        hash = hash.wrapping_mul(0x100_0000_01b3);
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roster() -> Vec<(u128, String)> {
        vec![(1, "Alex".into()), (2, "Sam".into()), (3, "Rex".into())]
    }

    #[test]
    fn the_command_tree_registers_every_form() {
        let commands = commands();
        let usages: Vec<&str> = commands.iter().map(|c| c.usage.as_str()).collect();
        assert!(usages.contains(&"speak"));
        assert!(usages.contains(&"speak invite <player:player>"));
        assert!(usages.contains(&"speak join <name:word>"));
        assert!(usages.contains(&"speak join <name:word> <password:word>"));
        assert!(usages.contains(&"speak leave"));
        assert!(usages.contains(&"speak create <name:word> <password:word> <type:word>"));
    }

    #[test]
    fn help_is_the_default_and_lists_the_forms() {
        let mut m = GroupManager::new(true);
        assert!(matches!(
            perform(&mut m, 1, "Alex", &[], &roster()),
            CommandReply::Tell(line) if line.contains("/speak")
        ));
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["help"], &roster()),
            CommandReply::Tell(_)
        ));
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["nonsense"], &roster()),
            CommandReply::Tell(_)
        ));
    }

    #[test]
    fn create_joins_and_reports_the_group() {
        let mut m = GroupManager::new(true);
        match perform(&mut m, 1, "Alex", &["create", "team"], &roster()) {
            CommandReply::GroupChanged { tell, group } => {
                assert!(tell.contains("team"), "{tell}");
                assert_eq!(group.as_deref(), Some("team"));
            }
            other => panic!("expected a group change, got {other:?}"),
        }
        // With a password and a type.
        match perform(
            &mut m,
            2,
            "Sam",
            &["create", "secret", "pw", "isolated"],
            &roster(),
        ) {
            CommandReply::GroupChanged { group, .. } => {
                assert_eq!(group.as_deref(), Some("secret"));
            }
            other => panic!("expected a group change, got {other:?}"),
        }
        assert_eq!(
            m.player_group_name(2).map(|_| ()),
            Some(()),
            "the creator is in"
        );
    }

    #[test]
    fn an_unknown_type_refuses_with_the_words() {
        let mut m = GroupManager::new(true);
        match perform(&mut m, 1, "Alex", &["create", "team", "", "loud"], &roster()) {
            CommandReply::Tell(line) => assert!(line.contains("open"), "{line}"),
            other => panic!("expected a refusal, got {other:?}"),
        }
    }

    #[test]
    fn join_and_leave_flow_through_the_group_manager() {
        let mut m = GroupManager::new(true);
        perform(&mut m, 1, "Alex", &["create", "team"], &roster());
        match perform(&mut m, 2, "Sam", &["join", "team"], &roster()) {
            CommandReply::GroupChanged { group, .. } => {
                assert_eq!(group.as_deref(), Some("team"));
            }
            other => panic!("expected a join, got {other:?}"),
        }
        match perform(&mut m, 2, "Sam", &["leave"], &roster()) {
            CommandReply::GroupChanged { tell, group } => {
                assert!(tell.contains("team"));
                assert_eq!(group, None);
            }
            other => panic!("expected a leave, got {other:?}"),
        }
        // Leaving with no group says so.
        assert!(matches!(
            perform(&mut m, 3, "Rex", &["leave"], &roster()),
            CommandReply::Tell(line) if line.contains("not in a group")
        ));
    }

    #[test]
    fn a_password_protected_join_needs_the_password() {
        let mut m = GroupManager::new(true);
        perform(&mut m, 1, "Alex", &["create", "club", "pw"], &roster());
        assert!(matches!(
            perform(&mut m, 2, "Sam", &["join", "club"], &roster()),
            CommandReply::Tell(line) if line.contains("password")
        ));
        assert!(matches!(
            perform(&mut m, 2, "Sam", &["join", "club", "nope"], &roster()),
            CommandReply::Tell(line) if line.contains("password")
        ));
        assert!(matches!(
            perform(&mut m, 2, "Sam", &["join", "club", "pw"], &roster()),
            CommandReply::GroupChanged { .. }
        ));
    }

    #[test]
    fn invites_resolve_names_case_insensitively_and_refuse_unknowns() {
        let mut m = GroupManager::new(true);
        perform(&mut m, 1, "Alex", &["create", "team"], &roster());
        match perform(&mut m, 1, "Alex", &["invite", "sam"], &roster()) {
            CommandReply::Tell(line) => {
                // The invite names the target as the inviter typed them,
                // and says what to type to accept.
                assert!(line.contains("sam"), "{line}");
                assert!(line.contains("/speak join team"), "the line says what to type");
            }
            other => panic!("expected an invite, got {other:?}"),
        }
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["invite", "nobody"], &roster()),
            CommandReply::Tell(line) if line.contains("no such player")
        ));
        // An inviter with no group is refused.
        assert!(matches!(
            perform(&mut m, 2, "Sam", &["invite", "alex"], &roster()),
            CommandReply::Tell(line) if line.contains("join a group")
        ));
    }

    #[test]
    fn the_group_list_shows_names_types_and_counts() {
        let mut m = GroupManager::new(true);
        perform(&mut m, 1, "Alex", &["create", "open-group"], &roster());
        perform(&mut m, 3, "Rex", &["create", "secret", "pw", "isolated"], &roster());
        perform(&mut m, 3, "Rex", &["join", "secret", "pw"], &roster());
        perform(&mut m, 1, "Alex", &["join", "open-group"], &roster());
        perform(&mut m, 2, "Sam", &["join", "open-group"], &roster());
        match perform(&mut m, 3, "Rex", &["group"], &roster()) {
            CommandReply::Tell(list) => {
                assert!(list.contains("open-group"), "{list}");
                assert!(list.contains("isolated"), "{list}");
                assert!(list.contains("[password]"), "{list}");
                assert!(list.contains("2 members"), "{list}");
            }
            other => panic!("expected a list, got {other:?}"),
        }
        // The empty list says so.
        let mut m = GroupManager::new(true);
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["group"], &roster()),
            CommandReply::Tell(line) if line.contains("no groups")
        ));
    }

    #[test]
    fn disabled_groups_refuse_the_group_forms() {
        let mut m = GroupManager::new(true);
        m.groups_enabled = false;
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["create", "team"], &roster()),
            CommandReply::Tell(line) if line.contains("disabled")
        ));
        assert!(matches!(
            perform(&mut m, 1, "Alex", &["join", "team"], &roster()),
            CommandReply::Tell(line) if line.contains("disabled")
        ));
    }

    #[test]
    fn the_group_id_is_deterministic_and_distinct() {
        let a = group_id(1, "team");
        let b = group_id(1, "team");
        let c = group_id(2, "team");
        let d = group_id(1, "other");
        assert_eq!(a, b, "the same pair, the same id");
        assert_ne!(a, c);
        assert_ne!(a, d);
    }
}
