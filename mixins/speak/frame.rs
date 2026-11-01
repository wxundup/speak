// Once a frame on the window: while push-to-talk is held in a world, capture
// real PCM frames and send them to the server, and play whatever arrived
// from it. Releasing the key, leaving the world or turning the mod off stops
// the capture and drops what was left of it. Without the audio feature (the
// headless build) no device is ever opened: the state and the queues behave
// the same.
fn speak_frame(game: &mut crate::game::Game<'_>) {
    use crate::game::official_speak as speak;
    // The lifecycle phases fire on the server's game, not the window's, so
    // the client resets its own voice state here: out of a world (or of the
    // mod) nothing is captured, played or shown.
    let in_world =
        game.native_on(speak::ID) && game.native_window() && game.native_player().is_some();
    if !in_world {
        horizon_client_render::official_speak_audio::stop();
        speak::closed();
        speak::note_capture(false);
        return;
    }
    // Chat and inventory keep playback, but release PTT so a key-up consumed
    // by their controls cannot reopen the microphone when they close.
    if !game.native_playing() {
        speak::key(false, false, |k| game.native_setting(speak::ID, k));
    }
    let wanted = speak::talking();
    let tick = speak::tick_local();
    speak::tick_players(tick);
    if wanted != speak::capture_on() {
        if let Err(error) = horizon_client_render::official_speak_audio::set_capture_enabled(wanted)
        {
            log::warn!("Speak audio: {error}");
        }
        // Retry a failed device open on the next hold, not every render frame.
        speak::note_capture(wanted);
    }
    if wanted {
        for frame in horizon_client_render::official_speak_audio::take_frames() {
            let packet = speak::MicPacket {
                data: speak::pcm_bytes(&frame),
                sequence: speak::next_sequence(),
                whisper: false,
                group: None,
            };
            if let Ok(bytes) = speak::encode_mic_packet(&packet) {
                let _ = game.native_net_send_to_server(speak::CHANNEL, &bytes);
            }
        }
    }

    // What the server sent reaches the speakers at the output volume,
    // whether the local player is talking or only listening.
    {
        let volume = {
            let g = &*game;
            speak::settings(|k| g.native_setting(speak::ID, k)).volume
        };
        for packet in speak::drain_playback() {
            let samples = speak::pcm_samples(&packet.data);
            let _ = horizon_client_render::official_speak_audio::enqueue_playback(&samples, volume);
        }
    }
}
