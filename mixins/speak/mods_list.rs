fn speak_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::speak_hooks as hooks;
    mods.push(crate::game::native_hooks::NativeMod {
        id: hooks::ID.into(),
        title: format!("Speak. {}", hooks::VERSION),
        description: hooks::DESCRIPTION.into(),
        keys: hooks::KEYS
            .iter()
            .map(|(name, label, default)| {
                [name.to_string(), label.to_string(), default.to_string()]
            })
            .collect(),
    });
}
