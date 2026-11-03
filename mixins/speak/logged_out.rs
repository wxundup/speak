fn speak_player_logged_out(
    game: &mut crate::game::Game<'_>,
    player: u128,
    name: &str,
) {
    use crate::game::speak_groups as groups;
    use crate::game::speak_hooks as hooks;
    use crate::game::speak_session as session;
    use crate::game::speak_states as states;
    if !game.native_on(hooks::ID) {
        return;
    }
    let _ = name;
    session::close(player);
    states::close(player);
    groups::on_disconnect(player);
}
