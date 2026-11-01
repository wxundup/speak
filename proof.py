"""Speak. evidence check: the mod loads and its settings page cycles.

Real PCM audio delivery is NOT asserted here. PCM delivery over the native
`speak:voice` channel is verified by `tools/native_voice_e2e.py` (three
clients, localhost dedicated engine, exact nearby delivery plus far and
cross-dimension isolation). This script checks only what is observable
without audio, mod load, settings cycle, and reports audio delivery as
outstanding, so a pass never implies audio success.

Speak. targets hooks contract 80 only; there is no hooks-62 compatibility
claim.
"""
import json
from game import Game

DATA = "horizon-mods/.horizon-mod-data/speak"
FILES = {
    f"{DATA}/mode": "pt",
    f"{DATA}/distance": "100",
    f"{DATA}/whisper_distance": "12",
    f"{DATA}/mic_sensitivity": "0.5",
    f"{DATA}/volume": "1.0",
}

EXPECTED_LABELS = [
    "Mode: push to talk",
    "Distance: 100 blocks",
    "Whisper: 12 blocks",
    "Mic sensitivity: 50",
    "Volume: 100",
]


def run(ctl, resources, out):
    evidence = {"proof": "speak"}
    with Game(ctl, resources, out, FILES) as g:
        g.run("options", "mods")
        assert "Speak. 0.1.0" in g.labels(), g.labels()
        evidence["mod_loaded"] = True

        g.run("click", g.ids()["Settings"])
        page = g.labels()
        assert page[:5] == EXPECTED_LABELS, page
        evidence["settings_cycle"] = True
        g.shot("settings-page")

    evidence["audio_delivery_verified"] = False
    evidence["audio_note"] = (
        "PCM wire codecs are pure functions needing no device; real mic capture "
        "and speaker playback (mixins/speak/audio.rs) sit behind the `audio` "
        "feature and `HORIZON_NO_AUDIO=1` disables them. Real PCM delivery over "
        "the native `speak:voice` channel is asserted by tools/native_voice_e2e.py "
        "(nearby delivery, far and cross-dimension isolation, Speak command tree, "
        "clean stop); it is not acoustic verification and opens no audio device. "
        "Audio delivery is otherwise a manual mic-and-listening test and is "
        "outstanding."
    )
    print(json.dumps(evidence, indent=2))
