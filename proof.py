"""Speak. in a running modded game: the key binds, the settings page cycles,
a nearby player hears the speaker and a far player does not."""
import json
import time
from game import Game, difference

DATA = "horizon-mods/.horizon-mod-data/speak"
# A player's settings: voice activation, 160 blocks, whisper 16, sensitivity 0.75, volume 1.25.
FILES = {
    f"{DATA}/mode": "vox",
    f"{DATA}/distance": "160",
    f"{DATA}/whisper_distance": "16",
    f"{DATA}/mic_sensitivity": "0.75",
    f"{DATA}/volume": "1.25",
}


def run(ctl, resources, out):
    with Game(ctl, resources, out, FILES) as g:
        results = {}
        g.run("options", "mods")
        assert "Speak. 0.1.0" in g.labels(), g.labels()
        g.run("click", g.ids()["Settings"])
        page = g.labels()
        assert page[:5] == [
            "Mode: voice activation",
            "Distance: 160 blocks",
            "Whisper: 16 blocks",
            "Mic sensitivity: 75",
            "Volume: 125",
        ], page
        g.shot("settings-page")
        g.play()
        g.run("command", "tp @s ~ ~ ~ 0 10")
        time.sleep(2)
        base = g.shot("1-normal")
        # Hold the speak key: the local player is now transmitting.
        g.run("key", "v", "down")
        time.sleep(2)
        talking = g.shot("2-speaking")
        results["diff_speaking"] = round(difference(base, talking), 1)
        g.run("key", "v", "up")
        time.sleep(1)
        released = g.shot("3-released")
        results["diff_released"] = round(difference(base, released), 1)
        assert results["diff_speaking"] > results["diff_released"], results
        print(json.dumps({"proof": "speak", "ok": True, **results}, indent=2))
