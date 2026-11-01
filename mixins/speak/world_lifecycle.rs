// The world opening and closing: start and stop the voice pipeline.
fn speak_world_lifecycle(phase: crate::game::native_lifecycle::WorldPhase) {
    use crate::game::native_lifecycle::WorldPhase;
    use crate::game::official_speak as speak;
    match phase {
        WorldPhase::WorldOpenReady => speak::opened(),
        WorldPhase::WorldOpenFailed => speak::opened(),
        WorldPhase::WorldCloseFinal => speak::closed(),
        WorldPhase::WorldCloseFailed => speak::closed(),
        _ => {}
    }
}
