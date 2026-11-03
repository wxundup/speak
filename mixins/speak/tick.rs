fn speak_tick(game: &mut crate::game::Game<'_>) {
    use crate::game::speak_client as client;
    use crate::game::speak_frame as frame;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_icons as icons;
    use crate::game::speak_mic as mic;
    use crate::game::speak_packets as packets;
    if !game.native_window() || !game.native_on(hooks::ID) {
        return;
    }
    // The fade advances with the tick count.
    icons::tick(frame::window_tick());
    if client::disabled(|k| game.native_setting(hooks::ID, k)) {
        mic::with_mic(|m| m.set_disabled(true));
        return;
    }
    mic::with_mic(|m| m.set_disabled(false));
    let frame_data: Option<Vec<i16>> = None;
    let muted = client::muted(|k| game.native_setting(hooks::ID, k));
    let whispering = client::whispering();
    let push_to_talk = frame::talking();
    let step = mic::with_mic(|m| m.tick(frame_data.as_deref(), muted, push_to_talk, whispering));
    match step {
        mic::MicStep::Send { data, .. } => {
            let _ = game.native_net_send_to_server(hooks::CHANNEL, &data);
        }
        mic::MicStep::Stop { .. } => {
            let stop = packets::encode_mic(&packets::MicPacket {
                whisper: false,
                sequence_number: 0,
                data: Vec::new(),
            });
            let _ = game.native_net_send_to_server(hooks::CHANNEL, &stop);
        }
        mic::MicStep::Idle => {}
    }
}
