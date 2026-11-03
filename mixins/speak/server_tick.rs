fn speak_server_tick(
    game: &mut crate::game::Game<'_>,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    use crate::game::native_lifecycle::ServerTickPhase;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_mix as mix;
    use crate::game::speak_packets as packets;
    use crate::game::speak_session as session;
    use crate::game::speak_states as states;
    if !matches!(phase, ServerTickPhase::ServerTickEnd { .. }) || !game.native_on(hooks::ID) {
        return;
    }
    let packets_in = session::drain_packets();
    if packets_in.is_empty() {
        return;
    }
    let players = game.native_server_players();
    let mix_players: Vec<mix::MixPlayer> = players
        .iter()
        .map(|p| mix::MixPlayer {
            uuid: p.uuid,
            name: p.name.clone(),
            dimension: p.dimension.clone(),
            position: p.position,
            game_mode: p.game_mode,
            disabled: states::effective_disabled(p.uuid).unwrap_or(false),
            force_muted: states::effective_muted(p.uuid).unwrap_or(false),
        })
        .collect();
    for (sender, data) in packets_in {
        let Some(packet) = packets::decode_mic(&data) else {
            continue;
        };
        let Some(state) = players.iter().find(|p| p.uuid == sender) else {
            continue;
        };
        let mix = mix::Mix {
            config: mix_config(game),
            players: &mix_players,
            same_group: &crate::game::speak_groups::same_group,
            sender_open: &crate::game::speak_groups::sender_open,
            sender_isolated: &crate::game::speak_groups::sender_isolated,
            receiver_isolated: &crate::game::speak_groups::receiver_isolated,
        };
        let Some(receivers) = mix.receivers(sender, packet.whisper) else {
            continue;
        };
        let sound = packets::encode_sound(&packets::SoundPacket {
            sound_kind: packets::SOUND_PLAYER,
            sender,
            channel_id: sender,
            sequence_number: packet.sequence_number as i64,
            whisper: packet.whisper,
            distance: mix.voice_distance(packet.whisper) as f32,
            position: state.position,
            dimension: state.dimension.clone(),
            name: state.name.clone(),
            data: packet.data,
        });
        for receiver in receivers {
            let target = match receiver {
                mix::Receiver::Group { player }
                | mix::Receiver::Proximity { player, .. }
                | mix::Receiver::Location { player } => player,
            };
            // Ok means accepted: a player who left or isn't on a Horizon
            // client doesn't get it.
            let _ = game.native_net_send_to_player(target, hooks::CHANNEL, &sound);
        }
    }
}

fn mix_config(game: &mut crate::game::Game<'_>) -> mix::MixConfig {
    use crate::game::speak_config as config;
    use crate::game::speak_hooks as hooks;
    let number = |key: &str, default: f64| {
        game.native_setting(hooks::ID, key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(default)
    };
    mix::MixConfig {
        max_voice_distance: number("distance", config::SERVER_MAX_VOICE_DISTANCE),
        whisper_distance: number("whisper_distance", config::SERVER_WHISPER_DISTANCE),
        broadcast_range: number("broadcast_range", config::SERVER_BROADCAST_RANGE),
        spectator_interaction: config::SERVER_SPECTATOR_INTERACTION,
        spectator_player_possession: config::SERVER_SPECTATOR_PLAYER_POSSESSION,
    }
}
