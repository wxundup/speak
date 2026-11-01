// A player's client sent a voice packet: check the channel and queue the
// bytes for the server's mix at the tick's end (`server_tick.rs`). Runs on
// the server's game, with no player's state swapped in.
fn speak_net_from_client(
    game: &mut crate::game::Game<'_>,
    player: u128,
    channel: &str,
    data: &[u8],
) {
    use crate::game::official_speak as speak;
    if !game.native_on(speak::ID) || channel != speak::CHANNEL {
        return;
    }
    // The mix decodes and rejects the malformed itself; the queue is bounded.
    speak::queue_packet(player, data.to_vec());
}
