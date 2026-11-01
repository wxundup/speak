# Speak. (Horizon native mod)

Speak. is a proximity voice chat for Horizon: hold a key to talk and the players near you hear you, or join a group to talk across distance. It is a [horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle (`mixins/speak/`) that the Horizon launcher builds into the game.

**Author:** wxund  
**Licence:** MIT  
**Source:** https://github.com/wxundup/speak

## Features

- **Proximity voice chat** — players hear you based on in-game distance, like Simple Voice Chat.
- **Push to talk** and **voice activation (VOX)** — pick your mode in settings.
- **Password protected groups** — talk to players outside your vicinity.
- **Whispering** — a shorter distance for quiet conversation.
- **Microphone and speaker test** — check your levels from the settings page.
- **Configurable voice distance** and mic sensitivity.
- **Individual player volume adjustment.**
- **3D sound** — voices pan to the speaker's position.
- **AES encryption** — voice packets are encrypted between client and server.
- **Commands:** `/speak invite <player>`, `/speak group <name>`, `/speak leave`, `/speak whisper`, `/speak pt`, `/speak vox`.
- **GUI overlay** — talking players are shown near their heads.
- **Settings page** — Options > Mods > Speak. > Settings.

## The design

Speak. is a port of the ideas in [Simple Voice Chat](https://github.com/henkelmax/simple-voice-chat) to Horizon's native mod system. Simple Voice Chat runs a separate UDP voice server (port 24454) and encodes microphone audio with Opus/RNNoise. Speak. follows the same architecture but expresses it as Horizon native hook points:

| Simple Voice Chat | Speak. (Horizon native) |
|---|---|
| Mic audio capture (OpenAL) | `native_player_tick` reads mic level; `native_net_from_client` sends it |
| Opus encode/decode | `runtime.rs` Opus frame stubs (to be wired to an audio codec) |
| UDP voice server (24454) | `native_server_tick` mixes proximity audio over `native_net_*` mod messages |
| Proximity / whisper distance | `runtime.rs` `in_range()` compares player positions each tick |
| Groups | `runtime.rs` `Group` type, named password-protected groups |
| GUI overlay (talking icons) | `world_drawn.rs` draws names above talking players |
| Settings screen (V key menu) | `settings.rs` / `settings_click.rs` in Options > Mods |
| Push-to-talk key (V) | `native_key` hook, bound to a player's key |

## Files

| File | What it is |
|---|---|
| `mixins/speak/horizon-mixin.toml` | The bundle: one module and the HEAD hooks |
| `mixins/speak/runtime.rs` | The mod's logic and state (settings, voice pipeline, proximity, groups, orchestration), also this crate's library, for its tests |
| `mixins/speak/key.rs`, `wheel.rs`, `mods.rs`, `settings.rs`, `settings_click.rs` | Client UI hooks (key bind, volume wheel, Mods row, settings page) |
| `mixins/speak/server_tick.rs`, `world_lifecycle.rs`, `register.rs`, `command.rs` | Server voice mixing, world lifecycle, and `/speak` commands |
| `mixins/speak/net_from_client.rs`, `net_from_server.rs` | Voice packets across the mod channel |
| `mixins/speak/player_tick.rs` | Per-player voice activity |
| `mixins/speak/render_needs.rs`, `world_drawn.rs` | Talking-player overlay |
| `native/tools/native.py` | Shared by the native mods: apply, check, build, package, proof |

## Build and prove

```sh
(cd native/speak && cargo test)                    # the logic
python3 native/tools/native.py check speak          # apply on the tested Horizon and cargo check the game
python3 native/tools/native.py check speak --horizon latest  # ... on the newest Nightly
python3 native/tools/native.py check all            # with the other native mods, as a player's set
python3 native/tools/native.py package speak        # native/target/speak-<version>.hnmod for the registry
python3 native/tools/native.py proof speak --resources /path/to/.local/horizon-client/26.3
```

The proof needs resources prepared from your own Minecraft 26.3 (`horizon-prepare`); none are in this repository. It seeds an earlier version's settings in a temporary game folder, joins a world with two players, presses the speak key, and checks that the nearby player's talking indicator appears and the far player does not.

## Install

In the Horizon launcher: Mods → Speak. → Install, then **Build** on the same page (once built, switch it on or off in the game under Options → Mods). The launcher downloads its own Rust and builds the game with your native mods; see Horizon's [native mods](https://github.com/horizon-rust/horizon/blob/main/docs/native-mods.md).

## Notes

- Native mods run with the player's account permissions and are not sandboxed. The first install shows a trust notice; building needs `--trust-native-code`.
- Speak. hooks only Horizon's native hook points, which keep their names and signatures across releases. `compatibility.json` names the hook API version it needs (`"hooks": 62`), so it builds on any Horizon with that version or later.
- The audio codec (Opus/RNNoise), 3D positional audio, and AES encryption are currently stubs in `runtime.rs` (empty `encode_mic_packet`/`decode_mic_packet`/`encode_sound_packet`/`decode_sound_packet`). Wire them to a real audio crate once it is in Horizon's `Cargo.lock`.
- License: MIT.
