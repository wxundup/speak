// A player's /speak command on the server: Speak.'s own, when it is one.
fn speak_perform_command(
    game: &mut crate::game::Game<'_>,
    player: &crate::game::native_server::NativeServerPlayer,
    command: &str,
    ran: &mut bool,
) {
    if !*ran {
        let Some(rest) = command.strip_prefix("speak ") else {
            return;
        };
        let mut parts = rest.split_whitespace();
        let sub = parts.next().unwrap_or("");
        match sub {
            "invite" => {
                if let Some(target) = parts.next() {
                    game.native_tell(player.uuid, &format!("Speak.: inviting {}", target));
                }
                *ran = true;
            }
            "group" => {
                if let Some(name) = parts.next() {
                    game.native_tell(player.uuid, &format!("Speak.: joining group {}", name));
                }
                *ran = true;
            }
            "leave" => {
                game.native_tell(player.uuid, "Speak.: left your group");
                *ran = true;
            }
            "whisper" => {
                game.native_tell(player.uuid, "Speak.: whispering (short distance)");
                *ran = true;
            }
            "pt" | "pushtotalk" => {
                game.native_tell(player.uuid, "Speak.: push to talk mode");
                *ran = true;
            }
            "vox" | "voiceactivation" => {
                game.native_tell(player.uuid, "Speak.: voice activation mode");
                *ran = true;
            }
            _ => {
                game.native_tell(
                    player.uuid,
                    "Speak.: /speak <invite|group|leave|whisper|pt|vox>",
                );
                *ran = true;
            }
        }
    }
}
