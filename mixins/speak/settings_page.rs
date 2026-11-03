fn speak_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::speak_client as client;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_icons as icons;
    use crate::game::speak_settings as settings;
    if mod_id != hooks::ID {
        return;
    }
    let game_ref = &*game;
    let get = |name: &str| game_ref.native_setting(hooks::ID, name);
    let number = |key: &str, default: f64| {
        get(key)
            .and_then(|v| v.trim().parse::<f64>().ok())
            .unwrap_or(default)
    };
    let flag = |key: &str| get(key).as_deref() == Some("true");
    let values = settings::Values {
        voice_volume: number(settings::settings_key::VOICE_VOLUME, 1.0),
        activation_ptt: get(settings::settings_key::ACTIVATION).as_deref() != Some("VOICE"),
        mic_gain: number(settings::settings_key::MIC_GAIN, 0.0),
        agc: flag(settings::settings_key::AGC),
        denoiser: flag(settings::settings_key::DENOISER),
        vad: flag(settings::settings_key::VAD),
        va_threshold: number(settings::settings_key::VA_THRESHOLD, -50.0),
        voice_delay: number(settings::settings_key::VOICE_DELAY, 25.0) as u32,
        ptt_delay: number(settings::settings_key::PTT_DELAY, 5.0) as u32,
        muted: flag(settings::settings_key::MUTED),
        disabled: flag(settings::settings_key::DISABLED),
        hide_icons: flag(settings::settings_key::HIDE_ICONS),
        player_volume: match icons::last_talker() {
            Some((uuid, _)) => client::player_volume(uuid, &get),
            None => 1.0,
        },
        player_name: icons::last_talker().map(|(_, name)| name),
    };
    buttons.extend(settings::labels(&values));
}

mod settings_key {
    pub use crate::game::speak_hooks::settings::*;
}
