// Native server settings are operator-only and refreshed after persistence.
fn speak_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    use crate::game::official_speak as speak;
    use horizon_client::text::Text;
    if *ran || !game.native_on(speak::ID) { return; }
    let Some(rest) = command.strip_prefix("speak") else { return; };
    if !rest.is_empty() && !rest.starts_with(' ') { return; }
    *ran = true;
    let tell = |game: &mut crate::game::Game<'_>, key: &str, args: &[&dyn std::fmt::Display]| {
        let catalog: serde_json::Value = serde_json::from_slice(
            horizon_client::native_data!("speak", "en_us.json")
        ).unwrap_or_default();
        let fallback = catalog[key].as_str().unwrap_or(key);
        let line = Text::tr(key, args).with_component_fields(vec![("fallback".into(), fallback.into())]);
        let _ = game.native_tell(player.uuid, line);
    };
    let mut words = rest.split_whitespace();
    match words.next() {
        None => { tell(game, "speak.usage", &[]); return; }
        Some("distance") => {}
        Some(_) => { tell(game, "speak.usage", &[]); return; }
    }
    if player.permission < 2 { tell(game, "commands.generic.permission", &[]); return; }
    let Some(distance) = words.next().and_then(|s| s.parse::<f32>().ok()).filter(|n| n.is_finite()) else {
        tell(game, "speak.distance.invalid", &[]); return;
    };
    let distance = speak::broadcast_range(distance);
    game.native_set_setting(speak::ID, "distance", &distance.to_string());
    speak::refresh_settings(|key| game.native_setting(speak::ID, key));
    tell(game, "speak.distance.set", &[&distance]);
}
