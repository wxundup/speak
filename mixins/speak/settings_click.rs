// A click on Speak.'s settings page: change the setting and keep it.
fn speak_click(game: &mut crate::game::Game<'_>, mod_id: &str, button: usize) {
    use crate::game::official_speak as speak;
    if mod_id != speak::ID {
        return;
    }
    let mut settings = speak::settings(|key| game.native_setting(speak::ID, key));
    settings.click(button);
    speak::set_settings(settings);
    for (key, value) in settings.stored() {
        game.native_set_setting(speak::ID, key, &value);
    }
}
