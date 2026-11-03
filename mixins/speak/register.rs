fn speak_register_commands(commands: &mut Vec<crate::game::native_server::NativeCommand>) {
    // The group commands (`VoicechatCommands`' set).
    for usage in [
        "speak",
        "speak help",
        "speak invite <player:player>",
        "speak join <name:word>",
        "speak join <name:word> <password:word>",
        "speak leave",
        "speak group",
        "speak create <name:word>",
        "speak create <name:word> <password:word>",
        "speak create <name:word> <password:word> <type:word>",
    ] {
        commands.push(crate::game::native_server::NativeCommand::new(usage));
    }
    // The menu's `/speakmenu` (the menu opens on the client).
    commands.push(crate::game::native_server::NativeCommand::new("speakmenu"));
}
