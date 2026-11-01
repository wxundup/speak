// A voice packet from a client: decode it and queue it for this tick's mix.
fn speak_net_from_client(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    channel: &str,
    data: &[u8],
) {
    use crate::game::official_speak as speak;
    if channel != speak::CHANNEL {
        return;
    }
    if !game.native_on(speak::ID) {
        return;
    }
    let Ok(packet) = speak::decode_mic_packet(data) else {
        return;
    };
    let _position = [0.0, 0.0, 0.0]; // TODO: read the player's position from the level
    speak::queue_packet(player.uuid, data.to_vec());
}
