// Speak.'s commands in every player's command tree.
fn speak_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    commands.push(crate::game::native_server::NativeCommand::new(
        "speak <subcommand> [args...]",
    ));
}
