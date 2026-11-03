# Speak. (Horizon native mod)

Speak. is a proximity voice chat for Horizon: hold a key to talk and the players near you hear you — or join a group and talk beyond distance.

**Author:** wxund  
**Licence:** MIT

## The keys

Six, all bindable in Options > Mods:

| Key | Default | What it does |
|---|---|---|
| Speak | Caps Lock | Hold to transmit |
| Menu | V | Open the voice menu |
| Mute | M | Toggle your microphone |
| Whisper | B | Hold, while transmitting, for the shorter distance |
| Hide icons | H | Toggle the HUD's indicators |
| Disable | N | Toggle the whole voice chat |

## The menu

Press V. A centered panel — the toggles and the quick settings, keyboard-navigated (arrows move, the number keys and Enter use a row, V or Esc closes):

- Mute microphone, Deafen (hear nobody), Hide icons, Voice chat
- Voice distance, Whisper distance, Volume
- Leave group — and the header shows the group you are in

## The indicators

Icons, not text, in a box in the top-left corner: a pixel speaker glyph for your own mic state — waves when talking, one wave when whispering, a slash when muted, struck through when deafened, bare and gray when the voice chat is off — and a glyph and name for each talking player, whisperers in their own colour, fading half a second after the last packet.

## Everything else

- **Proximity routing** — players hear you within voice range in the same dimension.
- **Groups** — `/speak group <name>` joins (or creates) a group and talks to its members whatever the distance; `/speak group <name> <password>` guards one; `/speak group <name> type isolated` makes one that hears no strangers and bleeds nothing; `/speak invite <player>`, `/speak leave`. A group that empties is deleted; a disconnect leaves it too.
- **Per-player volume** — the settings page shows the last heard player's own volume; a click steps it 25%, wrapping at 200%.
- **Settings page** — Options > Mods > Speak. > Settings: mode (push to talk / voice activation), the distances, mic sensitivity, volume, the toggles.
- **Commands** — `/speak`, `/speak invite <player>`, `/speak group <name> [password]`, `/speak group <name> type <open|isolated>`, `/speak leave`, `/speak pt`, `/speak vox`.

## Install

In the Horizon launcher: Mods → Speak. → Install, then **Build** on the same page (once built, switch it on or off in the game under Options → Mods).

## Notes

- Native mods run with the player's account permissions and are not sandboxed. The first install shows a trust notice.
- Only Horizon clients receive voice; vanilla clients connect normally and hear nothing.
- License: MIT.
