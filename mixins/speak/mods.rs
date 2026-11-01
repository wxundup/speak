// Speak.'s row in Options > Mods, with its key.
fn speak_listed(mods: &mut Vec<crate::game::native_hooks::NativeMod>) {
    use crate::game::official_speak as speak;
    mods.push(crate::game::native_hooks::NativeMod {
        id: speak::ID.into(),
        title: format!("Speak. {}", speak::VERSION),
        description: speak::DESCRIPTION.into(),
        keys: vec![[
            speak::KEY.into(),
            "Speak key".into(),
            speak::DEFAULT_KEY.into(),
        ]],
    });
}
