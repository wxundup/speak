fn speak_wheel(game: &crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    use crate::game::speak_client as client;
    use crate::game::speak_hooks as hooks;
    if !game.native_on(hooks::ID) {
        return;
    }
    let playing = game.native_playing();
    let game_ref = &*game;
    let get = |name: &str| game_ref.native_setting(hooks::ID, name);
    let set = |name: &str, value: &str| game_ref.native_set_setting(hooks::ID, name, value);
    // While the push-to-talk key is down, the local output volume.
    let talking = game_ref.native_setting(hooks::ID, "speak").is_some();
    if playing && talking {
        let volume = get("volume")
            .and_then(|v| v.trim().parse::<f32>().ok())
            .unwrap_or(1.0);
        let next = (volume + steps * 0.1).clamp(0.0, 2.0);
        set("volume", &next.to_string());
        *claimed = true;
        return;
    }
    // Otherwise the last heard player's own volume, if any.
    let Some((uuid, _)) = crate::game::speak_icons::last_talker() else {
        return;
    };
    let before = client::player_volume(uuid, &get);
    let after = client::adjust_player_volume(uuid, steps * 0.1, &get, &set);
    if (after - before).abs() > f32::EPSILON {
        *claimed = true;
    }
}
