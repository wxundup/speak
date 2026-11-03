fn speak_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::speak_client as client;
    use crate::game::speak_frame as frame;
    use crate::game::speak_menu as menu;
    use crate::game::speak_hooks as hooks;
    if !game.native_on(hooks::ID) {
        return;
    }
    let playing = game.native_playing();
    let game_ref = &*game;
    let setting = |name: &str| game_ref.native_setting(hooks::ID, name);
    let set = |name: &str, value: &str| game_ref.native_set_setting(hooks::ID, name, value);
    let bound = |(name, _, default): (&str, &str, &str)| {
        game_ref.native_mod_key(hooks::ID, name, default)
    };
    // While the menu is open, its keys are the game's.
    if menu::is_open() {
        // The menu key closes it again, as it opened it.
        if pressed && key == bound(menu::MENU_KEY) {
            menu::set_open(false);
            return;
        }
        let action = menu::key(key, pressed);
        apply_menu_action(game, action, &setting);
        return;
    }
    // Releases: the push-to-talk and whisper keys need theirs.
    if !pressed {
        if key == bound(hooks::KEYS[0]) {
            frame::key(playing, false, &setting);
        } else if key == bound(hooks::KEYS[3]) {
            client::on_key(
                &bound(hooks::KEYS[2]),
                &bound(hooks::KEYS[3]),
                key,
                false,
                playing,
                &setting,
                &set,
            );
        }
        return;
    }
    // The menu key opens the menu.
    if key == bound(menu::MENU_KEY) && playing {
        menu::set_open(true);
        return;
    }
    if key == bound(hooks::KEYS[0]) {
        frame::key(playing, true, &setting);
        return;
    }
    // The mute and whisper keys (the client states').
    let _ = client::on_key(
        &bound(hooks::KEYS[2]),
        &bound(hooks::KEYS[3]),
        key,
        true,
        playing,
        &setting,
        &set,
    );
    // The hide-icons and disable keys toggle their settings.
    if key == bound(hooks::KEYS[4]) && playing {
        let hidden = setting("hide_icons").as_deref() == Some("true");
        set("hide_icons", if hidden { "false" } else { "true" });
    }
    if key == bound(hooks::KEYS[5]) && playing {
        let disabled = setting("disabled").as_deref() == Some("true");
        set("disabled", if disabled { "false" } else { "true" });
    }
}

/// A menu action applied to the game (the settings files, the states, the
/// server): each row's click does what its settings-page twin does.
fn apply_menu_action(
    game: &mut crate::game::Game<'_>,
    action: crate::game::speak_menu::MenuAction,
    setting: &dyn Fn(&str) -> Option<String>,
) {
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_menu::MenuAction as A;
    match action {
        A::None | A::Close => {}
        A::ToggleMute => {
            let muted = crate::game::speak_client::muted(setting);
            game.native_set_setting(
                hooks::ID,
                "muted",
                if muted { "false" } else { "true" },
            );
        }
        A::ToggleDeafen => {
            let deafened = crate::game::speak_client::deafened(setting);
            crate::game::speak_client::set_deafened(!deafened, |name, value| {
                game.native_set_setting(hooks::ID, name, value)
            });
        }
        A::ToggleHideIcons => {
            let hidden = setting("hide_icons").as_deref() == Some("true");
            game.native_set_setting(
                hooks::ID,
                "hide_icons",
                if hidden { "false" } else { "true" },
            );
        }
        A::ToggleVoiceChat => {
            let disabled = crate::game::speak_client::disabled(setting);
            crate::game::speak_client::set_disabled(!disabled, |name, value| {
                game.native_set_setting(hooks::ID, name, value)
            });
        }
        A::CycleDistance => cycle_setting(game, "distance", 50.0, 8.0, 1024.0),
        A::CycleWhisperDistance => cycle_setting(game, "whisper_distance", 5.0, 8.0, 1024.0),
        A::CycleVolume => cycle_setting(game, "volume", 0.25, 0.0, 2.0),
        A::LeaveGroup => {
            use crate::game::speak_packets as packets;
            let leave = packets::encode_control(packets::CONTROL_LEAVE, &[]);
            let _ = game.native_net_send_to_server(hooks::CHANNEL, &leave);
        }
    }
}

/// Steps a numeric setting and keeps it (the settings page's cycle, in
/// one place).
fn cycle_setting(
    game: &mut crate::game::Game<'_>,
    name: &str,
    stride: f32,
    lo: f32,
    hi: f32,
) {
    use crate::game::speak_hooks as hooks;
    let current = game
        .native_setting(hooks::ID, name)
        .and_then(|v| v.trim().parse::<f32>().ok())
        .unwrap_or(lo);
    let next = current + stride;
    let next = if next > hi { lo } else { next };
    game.native_set_setting(hooks::ID, name, &next.to_string());
}
