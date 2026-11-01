// Ask for the entity geometry split so the talking overlay can place names.
fn speak_render_needs(needs: &mut crate::game::native_render::NativeRenderNeeds) {
    use crate::game::official_speak as speak;
    if !speak::overlay_enabled() {
        return;
    }
    needs.entity_spans = true;
    needs.world_depth = true;
}
