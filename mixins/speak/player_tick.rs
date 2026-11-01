// Each living player ticks: read the mic level and, if the player is talking,
// queue a voice packet for the server to mix.
fn speak_player_tick(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
) {
    use crate::game::official_speak::{Mode, settings, talking, set_mic_level};
    if !game.native_on(crate::game::official_speak::ID) {
        return;
    }
    let level = 0.0f32; // TODO: read the mic level from audio capture (audio.rs)
    set_mic_level(level);

    let talking = match settings(|k| game.native_setting(crate::game::official_speak::ID, k)).mode {
        Mode::PushToTalk => talking(),
        Mode::VoiceActivation => level > settings(|k| game.native_setting(crate::game::official_speak::ID, k)).mic_sensitivity,
    };
    if talking {
        let frame = crate::game::official_speak::encode_mic_packet(level);
        game.native_net_send_to_server(crate::game::official_speak::CHANNEL, &frame);
    }
}
