fn speak_world_lifecycle(phase: crate::game::native_lifecycle::WorldPhase) {
    use crate::game::native_lifecycle::WorldPhase;
    use crate::game::speak_client as client;
    use crate::game::speak_frame as frame;
    use crate::game::speak_groups as groups;
    use crate::game::speak_menu as menu;
    use crate::game::speak_session as session;
    use crate::game::speak_states as states;
    use crate::game::speak_talk as talk;
    match phase {
        WorldPhase::WorldOpenReady | WorldPhase::WorldOpenFailed => {
            session::reset();
            groups::reset();
            menu::reset();
            talk::reset();
            frame::reset_sequence();
        }
        WorldPhase::WorldCloseFinal | WorldPhase::WorldCloseFailed => {
            states::reset();
            session::reset();
            groups::reset();
            menu::reset();
            talk::reset();
            client::closed();
            frame::closed();
            frame::reset_sequence();
        }
    }
}
