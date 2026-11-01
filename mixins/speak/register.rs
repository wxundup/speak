fn speak_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    use crate::game::native_server::NativeCommand;
    commands.push(NativeCommand::new("speak"));
    commands.push(NativeCommand::op("speak distance <blocks:number>", 2));
}
