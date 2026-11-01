// Speak.'s settings page: the mode, distance, whisper, mic sensitivity, volume.
fn speak_page(game: &mut crate::game::Game<'_>, mod_id: &str, buttons: &mut Vec<String>) {
    use crate::game::official_speak as speak;
    if mod_id != speak::ID {
        return;
    }
    let game = &*game;
    let settings = speak::settings(|key| game.native_setting(speak::ID, key));
    buttons.extend(settings.labels());
}
