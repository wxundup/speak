// The speak key went down or up: the player's choice of key, bound like the
// WebAssembly mods kept it (key.keyboard.v by default).
fn speak_key(game: &mut crate::game::Game<'_>, key: &str, pressed: bool) {
    use crate::game::official_speak as speak;
    // Turned off in Options > Mods: nothing until the next build leaves it out.
    if !game.native_on(speak::ID) {
        return;
    }
    let bound = game.native_mod_key(speak::ID, speak::KEY, speak::DEFAULT_KEY);
    if key != bound {
        return;
    }
    let playing = game.native_playing();
    let game = &*game;
    speak::key(playing, pressed, |name| {
        game.native_setting(speak::ID, name)
    });
}
