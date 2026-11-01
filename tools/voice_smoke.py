"""Speak. voice smoke: launch the actual dedicated engine on localhost, assert it
becomes ready and stops cleanly.

Server side only. Real PCM audio delivery between two clients is NOT covered
here: the mic / sound packet codecs are tested separately, so no voice audio is delivered.
A two-client synthetic PCM test and a human mic-and-listening test are
outstanding and must not be faked.

When run it uses fresh disposable mktemp homes, HORIZON_NO_AUDIO=1 for the
server, and HORIZON_LAUNCHER_NO_SELF_UPDATE=1; it binds 127.0.0.1 only and
touches no player worlds or accounts.
"""
import json
import os
import socket
import struct
import subprocess
import tempfile
import threading
import time
from pathlib import Path

root = Path(__file__).resolve().parent.parent
scratch = Path(tempfile.mkdtemp(prefix="horizon-speak-voice-smoke-"))

env = os.environ.copy()
for key, name in {
    "HORIZON_HOME": "game",
    "HORIZON_LAUNCHER_HOME": "launcher",
    "XDG_DATA_HOME": "data",
    "XDG_CONFIG_HOME": "config",
    "HORIZON_LAUNCHER_TRASH": "trash",
    "HORIZON_LAUNCHER_CLIPBOARD": "clipboard",
    "HORIZON_LAUNCHER_DESKTOP": "desktop",
}.items():
    env[key] = str(scratch / name)
env.update(HORIZON_NO_AUDIO="1", HORIZON_LAUNCHER_NO_SELF_UPDATE="1")

server = scratch / "server"
server.mkdir()
(server / "eula.txt").write_text("eula=true\n")

with socket.socket() as s:
    s.bind(("127.0.0.1", 0))
    port = s.getsockname()[1]
(server / "server.properties").write_text(
    "server-ip=127.0.0.1\n"
    f"server-port={port}\n"
    "online-mode=false\n"
    "white-list=false\n"
    "level-name=speak-voice-smoke\n"
    "level-seed=12345\n"
    "view-distance=2\n"
    "simulation-distance=2\n"
    "max-players=2\n"
    "spawn-protection=0\n"
    "pause-when-empty-seconds=-1\n"
)

resources = Path(
    os.environ.get(
        "HORIZON_SPEAK_RESOURCES",
        Path.home()
        / "Library/Application Support/horizon-launcher/versions/nightly-2026-11-01-4aa6854/.local/horizon-client/26.3",
    )
)
engine = root / "target/debug/horizon-client"
log = (scratch / "server-console.log").open("w")
process = subprocess.Popen(
    [str(engine), "--resources", str(resources), "--dedicated", "--nogui"],
    cwd=server,
    env=env,
    stdin=subprocess.PIPE,
    stdout=subprocess.PIPE,
    stderr=subprocess.STDOUT,
    text=True,
    bufsize=1,
)
lines = []


def reader():
    for line in process.stdout:
        lines.append(line.rstrip())
        log.write(line)
        log.flush()


thread = threading.Thread(target=reader, daemon=True)
thread.start()


def vi(n):
    out = bytearray()
    while n > 127:
        out.append((n & 127) | 128)
        n >>= 7
    return bytes(out) + bytes([n])


def readvi(sock):
    n = 0
    for i in range(5):
        b = sock.recv(1)
        if not b:
            raise EOFError()
        n |= (b[0] & 127) << (i * 7)
        if b[0] < 128:
            return n
    raise ValueError("bad varint")


def readn(sock, n):
    out = b""
    while len(out) < n:
        b = sock.recv(n - len(out))
        if not b:
            raise EOFError()
        out += b
    return out


def send(sock, data):
    sock.sendall(vi(len(data)) + data)


result = {
    "engine": str(engine),
    "bind": "127.0.0.1",
    "port": port,
    "audio_delivered": False,
}
try:
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        if process.poll() is not None:
            raise RuntimeError(f"engine exited {process.returncode}")
        if any("Done (" in x for x in lines):
            break
        time.sleep(0.1)
    else:
        raise TimeoutError("server did not become ready in 120 s")

    with socket.create_connection(("127.0.0.1", port), timeout=5) as s:
        host = b"127.0.0.1"
        send(s, b"\x00" + vi(777) + vi(len(host)) + host + struct.pack(">H", port) + b"\x01")
        send(s, b"\x00")
        packet = readn(s, readvi(s))
        assert packet[0] == 0
        n = 0
        shift = 0
        i = 1
        while True:
            b = packet[i]
            i += 1
            n |= (b & 127) << shift
            if b < 128:
                break
            shift += 7
        status = json.loads(packet[i : i + n])
        result["status"] = status
        assert status["version"]["protocol"] == 777, status
        assert status["players"]["online"] == 0, status
        stamp = struct.pack(">q", 123456789)
        send(s, b"\x01" + stamp)
        assert readn(s, readvi(s)) == b"\x01" + stamp
        result["status_and_ping"] = "passed"

    offset = len(lines)
    for command in ["list", "stop"]:
        process.stdin.write(command + "\n")
        process.stdin.flush()
        time.sleep(0.5)
    process.wait(timeout=30)
    thread.join(timeout=3)
    result["exit_code"] = process.returncode
    assert process.returncode == 0, result
    result["console_tail"] = lines[offset:]
    result["clean_stop"] = "passed"
except Exception as exc:
    result["error"] = str(exc)
    if process.poll() is None:
        try:
            process.stdin.write("stop\n")
            process.stdin.flush()
            process.wait(timeout=10)
        except Exception:
            process.kill()
            process.wait()
    raise
finally:
    (scratch / "voice-smoke-result.json").write_text(json.dumps(result, indent=2))
    log.close()
    print(json.dumps(result, indent=2))
