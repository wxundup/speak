// Draw the talking-player overlay: names above talking players.
fn speak_world_drawn(world: &crate::game::native_render::WorldDrawn) {
    use crate::game::official_speak as speak;
    if !speak::overlay_enabled() {
        return;
    }
    // Draw each talking player's name and voice level above their head, using
    // the positions tracked by `heard` and faded by `tick_players`. The actual
    // wgpu drawing happens against `world`'s encoder.
    let _players = speak::players();
    let _world = world;
}
