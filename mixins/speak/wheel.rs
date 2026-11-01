// While the key is held, the wheel nudges the local output volume instead of
// moving the hotbar.
fn speak_wheel(game: &crate::game::Game<'_>, steps: f32, claimed: &mut bool) {
    use crate::game::official_speak as speak;
    if game.native_on(speak::ID) && speak::wheel(steps) {
        *claimed = true;
    }
}
