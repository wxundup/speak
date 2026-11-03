fn speak_net_from_client(
    game: &mut crate::game::Game<'_>,
    player: u128,
    channel: &str,
    data: &[u8],
) {
    use crate::game::speak_groups as groups;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_packets as packets;
    use crate::game::speak_session as session;
    use crate::game::speak_states as states;
    if channel != hooks::CHANNEL || !game.native_on(hooks::ID) {
        return;
    }
    if let Some((control, _)) = packets::decode_control(data) {
        if control == packets::CONTROL_LEAVE {
            groups::leave(player);
            states::set_group(player, None);
            // Tell the client its group is gone, so its menu updates.
            let info = packets::encode_control(
                packets::CONTROL_GROUP,
                &packets::group_payload(groups::player_group_name(player).as_deref()),
            );
            let _ = game.native_net_send_to_player(player, hooks::CHANNEL, &info);
        }
        return;
    }
    session::queue_packet(player, data.to_vec());
}
