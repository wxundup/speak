fn speak_net_from_server(
    game: &mut crate::game::Game<'_>,
    channel: &str,
    data: &[u8],
) {
    use crate::game::speak_client as client;
    use crate::game::speak_frame as frame;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_icons as icons;
    use crate::game::speak_menu as menu;
    use crate::game::speak_packets as packets;
    use crate::game::speak_talk as talk;
    if channel != hooks::CHANNEL || !game.native_on(hooks::ID) {
        return;
    }
    if let Some((control, payload)) = packets::decode_control(data) {
        if control == packets::CONTROL_GROUP {
            menu::set_group(packets::parse_group(payload));
        }
        return;
    }
    // A deafened player hears nobody (`ClientConfig.disabled`'s hearing
    // twin).
    if client::deafened(|k| game.native_setting(hooks::ID, k)) {
        return;
    }
    let Some(packet) = packets::decode_sound(data) else {
        return;
    };
    talk::update(
        packet.sender,
        packet.whisper,
        &packet.data,
        frame::window_tick() * 50,
        |frame_data| frame::get_highest_audio_level(frame_data),
    );
    icons::heard(
        packet.sender,
        &packet.name,
        &packet.dimension,
        packet.position,
        packet.whisper,
        packet.range,
        frame::window_tick(),
    );
}
