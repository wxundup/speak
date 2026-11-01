// A voice packet from the server: decode it and show it in the overlay.
fn speak_net_from_server(
    game: &mut crate::game::Game<'_>,
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
    let Ok(packet) = speak::decode_sound_packet(data) else {
        return;
    };
    let tick = 0u64; // TODO: read the current tick from the level
    speak::heard(
        packet.sender,
        packet.sender_name,
        packet.dimension,
        packet.position,
        packet.voice_level,
        tick,
    );
}
