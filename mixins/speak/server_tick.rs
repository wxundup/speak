// The end of each server tick: mix the voice packets players sent this tick
// and send each one to the players near the speaker, in the same dimension.
// The server's own player table is the authority: what a client said about
// its position or name is never read, and the speaker never hears itself.
fn speak_server_tick(
    game: &mut crate::game::Game<'_>,
    _context: &crate::game::native_lifecycle::NativeLifecycleContext,
    phase: crate::game::native_lifecycle::ServerTickPhase,
) {
    use crate::game::native_lifecycle::ServerTickPhase;
    use crate::game::official_speak as speak;
    if !matches!(phase, ServerTickPhase::ServerTickEnd { .. }) {
        return;
    }
    if !game.native_on(speak::ID) {
        return;
    }
    let tick = speak::tick_server();
    let settings = {
        let g = &*game;
        speak::refresh_settings(|k| g.native_setting(speak::ID, k))
    };
    let packets = speak::drain_packets();
    let players = game.native_server_players();

    for (uuid, data) in packets {
        let Some(packet) = speak::decode_mic_packet(&data) else {
            continue; // malformed, oversized or trailing: dropped in silence
        };
        let Some(sender) = players.iter().find(|p| p.uuid == uuid) else {
            continue;
        };
        let sound = speak::SoundPacket {
            sender: sender.uuid,
            sender_name: sender.name.clone(),
            dimension: sender.dimension.clone(),
            position: sender.position,
            sequence: packet.sequence,
            data: packet.data.clone(),
        };
        let bytes = match speak::encode_sound_packet(&sound) {
            Ok(bytes) => bytes,
            Err(_) => continue,
        };
        for receiver in &players {
            if receiver.uuid == uuid {
                continue; // no echo to the speaker
            }
            if !speak::routes(
                &receiver.dimension,
                receiver.position,
                &sender.dimension,
                sender.position,
                packet.whisper,
                &settings,
            ) {
                continue; // another dimension, or out of range
            }
            let _ = game.native_net_send_to_player(receiver.uuid, speak::CHANNEL, &bytes);
        }
    }

    speak::tick_players(tick);
}
