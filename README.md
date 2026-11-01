# Speak. (Horizon native mod)

Speak. is a proximity voice chat for Horizon: hold a key to talk and the players near you hear you. It is a [horizon-mixins](https://github.com/horizon-rust/horizon-mixins) bundle (`mixins/speak/`) that the Horizon launcher builds into the game.

**Author:** wxund  
**Licence:** MIT  
**Source:** https://github.com/wxundup/speak

## MVP scope (approved)

- **Push to talk**, hold V to transmit, release to stop. A new hold restarts the frame stream. PTT only: the settings page may still offer VOX, but Speak. makes no automatic-microphone-activation guarantee.
- **Real PCM audio**, uncompressed mono PCM16, 48 kHz, 20 ms frames (960 samples, 1 920 bytes). One speaker is ~96,000 bytes/sec before headers (48 000 samples/s × 2 bytes = 50 × 1 920-byte frames/s). The wire codec (`encode_mic_packet` / `decode_mic_packet` / `encode_sound_packet` / `decode_sound_packet`) is pure PCM framing: no Opus, no RNNoise.
- **Proximity routing**, the server routes a speaker's PCM to listeners in the same dimension within range (`routes()`), using the server's own player table; what a client claims about its position is not trusted.

Not in the MVP (do not expect them): groups and passwords (the group bit is refused), the GUI overlay / talking indicators drawn above heads (state tracked, not rendered), 3D positional sound, a microphone-and-speaker level test UI, and AES or any custom encryption.

## Network

Voice travels on the Horizon native mod channel `speak:voice` (`game.native_net_send_to_player`). Encryption is the game's existing encryption, used only where it is enabled, Speak. adds no custom crypto and makes no encryption claim of its own.

Client audio is optional: a headless or dedicated build runs the same `mixins/speak/` bundle with no microphone or speaker; the server mix is proximity only. Real capture and playback live in `mixins/speak/audio.rs`, a cpal adapter behind the `audio` feature; `HORIZON_NO_AUDIO=1` makes opening a device return an error instead of manufacturing silence.

## Files

| File | What it is |
|---|---|
| `mixins/speak/horizon-mixin.toml` | The bundle: the `runtime` module and the HEAD hooks |
| `mixins/speak/runtime.rs` | The mod's logic and state (settings, key, PCM wire codec, proximity mix, tests) |
| `mixins/speak/audio.rs` | Microphone capture and speaker playback (cpal adapter, behind the `audio` feature) |
| `mixins/speak/net_client.rs`, `net_server.rs`, `frame.rs` | Restored: client voice-packet ingestion, server voice-packet delivery, per-frame capture/mix/play injection |
| `mixins/speak/key.rs`, `wheel.rs`, `mods.rs`, `settings.rs`, `settings_click.rs` | Client UI hooks (key bind, volume nudge, Mods row, settings page) |
| `mixins/speak/server_tick.rs`, `world_lifecycle.rs`, `register.rs`, `command.rs` | Server tick mix, world lifecycle, `/speak` commands |

## Build and prove

Standalone (this repo, no Horizon source, no audio device):

```sh
cargo test --all-targets --all-features   # the mod's logic: settings, PCM conversion, proximity, packet queue, no audio device
```

This builds only the standalone runtime crate (`mixins/speak/runtime.rs` as a lib, plus `mixins/speak/audio.rs` as its own test target). It does **not** compile the full game or the launcher.

A real native game build needs the bundle applied to Horizon source, then:

```sh
# server (dedicated, no audio device)
cargo build -p horizon-client --bin horizon-client --no-default-features
# client (windowed, default features incl. audio)
cargo build -p horizon-client --bin horizon-client
```

or, through the launcher, `native-build REPO_ROOT` with `--server-output freshDir`.

The bundle is matched to Horizon by hook contract, not by file layout:
- `horizon-native.json` at the repo root: hooks contract `80`, version `0.1.0`, tested fork revision `fcae565042e694cb49dcc024bb6d255572bf1a64` (the fork game it was tested on).
- The raw bundle `mixins/speak/` hardpins that same tested revision in `horizon-mixin.toml`; a packaged `repoRoot` carries the hooks contract `80`.

There is no claim of any old hooks-62 compatibility: Speak. targets hooks 80 only.

## Commands

Native PCM routing test (three protocol clients):

```sh
python3 tools/native_voice_e2e.py \
  --engine <path-to-horizon-client> \
  --resources <prepared-26.3-resources> \
  --horizon <horizon-source-root> \
  [--target-dir <cargo-target-dir>] \
  [--out <evidence-dir>]
```

It builds a Rust protocol fixture (`tools/native_voice_client.rs`), joins three clients to a localhost dedicated engine, sends real PCM through the native `speak:voice` channel, and asserts:
- exact nearby delivery (speaker to nearby listener only),
- far and cross-dimension isolation (distant and nether listeners hear nothing),
- the Speak command tree is present in every client's command tree,
- operator permission checks and immediate distance changes,
- translated command feedback and a clean engine stop.

No audio device is opened and this is not acoustic verification: it proves delivered PCM bytes, not sound.

## Acceptance matrix

| Check | How | Status |
|---|---|---|
| Standalone unit tests | `cargo test --all-targets --all-features` | pass |
| PCM conversion without devices | `pcm_bytes` / `pcm_samples` / `pcm_level` unit tests | pass |
| Headless compile | `cargo build -p horizon-client --bin horizon-client --no-default-features` (dedicated) | pass |
| Full-client compile | `cargo build -p horizon-client --bin horizon-client` (defaults) | pass |
| Real synthetic PCM routing | `native_voice_e2e.py` three-client localhost engine, six phases | pass |
| Launcher server export | Actual `build_server` pipeline, followed by the same six network phases using exported resources | pass (warm toolchain and source cache) |
| Real mic and listening | human test with actual microphone and speakers | outstanding (manual) |

## Install

In the Horizon launcher: Mods → Speak. → **Build**, then switch it on or off in the game under Options → Mods. The launcher downloads its own Rust and builds the game with your native mods; see Horizon's [native mods](https://github.com/horizon-rust/horizon/blob/main/docs/native-mods.md).

Build paths are `mixins/speak/` (the bundle and its `horizon-mixin.toml`), not a separate `native/` tree. Native mods run with the player's account permissions and are not sandboxed: the first install shows a trust notice and building needs `--trust-native-code`. Prepared 26.3 resources (`horizon-prepare`, from your own Minecraft install) are read-only and never committed. Tests run on `127.0.0.1` only, in disposable `mktemp` homes, with `HORIZON_NO_AUDIO=1` and `HORIZON_LAUNCHER_NO_SELF_UPDATE=1`; no player worlds or accounts are touched.

## Notes

- Native mods run with the player's account permissions and are not sandboxed.
- Speak. hooks only Horizon's native hook points (hooks contract 80), which keep their names and signatures across releases. `compatibility.json` names the hook API version it needs (`"hooks": 80`); the bundle `horizon-mixin.toml` hardpins the tested revision `fcae565`. There is no hooks-62 compatibility claim.
- Groups/passwords, the GUI overlay, 3D sound, Opus/RNNoise and custom AES are not implemented. The MVP wire format is raw PCM16.
- Existing game encryption is used only where it is enabled; Speak. adds no custom crypto.
- License: MIT.

### MVP limits

The automated network test uses one synthetic transmitter and three real protocol clients. It proves exact audio-byte delivery and server routing, not microphone permission or acoustic output. A human microphone-and-listening check remains required. Multiple simultaneous talkers currently append to one bounded output queue rather than being mixed; overlapping speech and device hot-plug recovery are follow-up work.
