// The end of each server tick: mix incoming voice and send it to listeners.
fn speak_server_tick(
    game: &mut crate::game::Game<'_>,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    use crate::game::native_lifecycle::ServerTickPhase;
    use crate::game::official_speak as speak;
    if !matches!(
        phase,
        crate::game::native_lifecycle::ServerTickPhase::ServerTickEnd { .. }
    ) {
        return;
    }
    if !game.native_on(speak::ID) {
        return;
    }
    let settings = speak::settings(|k| game.native_setting(speak::ID, k));
    let tick = 0u64; // TODO: read the current tick from the level

    let packets = speak::drain_packets();
    let players = game.native_server_players();

    for (uuid, data) in packets {
        let Some(packet) = speak::decode_mic_packet(&data) else {
            continue;
        };
        let Some(state) = players.iter().find(|p| p.uuid == uuid).cloned() else {
            continue;
        };
        let distance = if packet.whisper {
            settings.whisper_distance
        } else {
            settings.distance
        };
        let range = speak::broadcast_range(distance);

        for receiver in &players {
            if receiver.uuid == uuid {
                continue;
            }
            if !speak::in_range(receiver.position, state.position, range) {
                continue;
            }
            let sound = speak::encode_sound_packet(
                uuid,
                state.name.clone(),
                packet.data.clone(),
                packet.sequence,
                packet.whisper,
                range,
            );
            game.native_net_send_to_player(receiver.uuid, speak::CHANNEL, &sound);
        }
    }

    speak::tick_players(tick);
}
