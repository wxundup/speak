// The server sent a player's voice: remember who spoke, where and how loud
// (the level measured from the real PCM, never claimed by the sender), and
// queue the frame for the next window frame to play (`frame.rs`). Runs on
// the window's game only.
fn speak_net_from_server(game: &mut crate::game::Game<'_>, channel: &str, data: &[u8]) {
    use crate::game::official_speak as speak;
    if !game.native_on(speak::ID) || channel != speak::CHANNEL {
        return;
    }
    let Some(packet) = speak::decode_sound_packet(data) else {
        return; // malformed, oversized or trailing: not played
    };
    let level = speak::pcm_level(&packet.data);
    let tick = speak::tick_local();
    speak::heard(
        packet.sender,
        packet.sender_name.clone(),
        packet.dimension.clone(),
        packet.position,
        level,
        tick,
    );
    speak::queue_playback(packet);
}
