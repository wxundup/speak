fn speak_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    use crate::game::speak_commands as commands;
    use crate::game::speak_groups as groups;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_packets as packets;
    use crate::game::speak_states as states;
    use horizon_client::text::{RED, Text};
    if *ran || !game.native_on(hooks::ID) {
        return;
    }
    let mut words = command.split_whitespace();
    if words.next() != Some("speak") {
        return;
    }
    // `/speakmenu` opens the menu on the sender's client.
    if words.next() == Some("menu") {
        let info = packets::encode_control(packets::CONTROL_GROUP, &packets::group_payload(None));
        let _ = info;
        // The menu's open rides a control frame the client half reads.
        let open = packets::encode_control(packets::CONTROL_HELLO, &[]);
        let _ = game.native_net_send_to_player(player.uuid, hooks::CHANNEL, &open);
        *ran = true;
        return;
    }
    let words: Vec<&str> = words.collect();
    let roster: Vec<(u128, String)> = game
        .native_server_players()
        .into_iter()
        .map(|p| (p.uuid, p.name))
        .collect();
    let before = groups::player_group_name(player.uuid);
    let reply = {
        let mut manager = groups::GroupManager::new(true);
        let _ = &mut manager;
        // The shared table drives the real state.
        crate::game::speak_groups::with_groups(|manager| {
            commands::perform(manager, player.uuid, &player.name, &words, &roster)
        })
    };
    let after = groups::player_group_name(player.uuid);
    // The group change rides a control frame, so the client's menu shows it.
    if before != after {
        states::set_group(player.uuid, after.clone());
        let info = packets::encode_control(
            packets::CONTROL_GROUP,
            &packets::group_payload(after.as_deref()),
        );
        let _ = game.native_net_send_to_player(player.uuid, hooks::CHANNEL, &info);
    }
    match reply {
        commands::CommandReply::Tell(line) => {
            let _ = game.native_tell(player.uuid, Text::literal(line));
            *ran = true;
        }
        commands::CommandReply::GroupChanged { tell, .. } => {
            let _ = game.native_tell(player.uuid, Text::literal(tell));
            *ran = true;
        }
        commands::CommandReply::NotMine => {}
    }
}
