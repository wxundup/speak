#!/usr/bin/env python3
"""Build a tiny protocol client and prove real native-channel PCM proximity routing.
No microphone or speaker is opened. A pass proves delivered PCM bytes, not acoustic audio.
"""
import argparse
import json
import os
from pathlib import Path
import queue
import socket
import subprocess
import tempfile
import threading
import time


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--engine', required=True, type=Path)
    parser.add_argument('--resources', required=True, type=Path)
    parser.add_argument('--horizon', required=True, type=Path)
    parser.add_argument('--target-dir', type=Path)
    parser.add_argument('--out', type=Path)
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    root = args.out.resolve() if args.out else Path(tempfile.mkdtemp(prefix='speak-native-e2e-')).resolve()
    root.mkdir(parents=True, exist_ok=True)
    server = root / 'server'
    server.mkdir(exist_ok=False)
    env = os.environ.copy()
    for key in ['HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME',
                'HORIZON_LAUNCHER_TRASH', 'HORIZON_LAUNCHER_CLIPBOARD', 'HORIZON_LAUNCHER_DESKTOP']:
        env[key] = str(root / key.lower())
    env.update(HORIZON_NO_AUDIO='1', HORIZON_LAUNCHER_NO_SELF_UPDATE='1')
    fixture = root / 'fixture'
    fixture.mkdir()
    manifest = ('[package]\nname="speak-native-voice-fixture"\nversion="0.1.0"\nedition="2024"\n'
                '[workspace]\n[dependencies]\nhorizon-protocol={path=' + json.dumps(str(args.horizon.resolve() / 'crates/horizon-protocol')) + '}\n'
                'horizon-mod-speak={path=' + json.dumps(str(repo)) + '}\n'
                '[[bin]]\nname="voice-client"\npath=' + json.dumps(str(repo / 'tools/native_voice_client.rs')) + '\n')
    (fixture / 'Cargo.toml').write_text(manifest)
    target = (args.target_dir or root / 'target').resolve()
    with (root / 'fixture-build.log').open('w') as log:
        subprocess.run(['cargo', 'build', '--manifest-path', str(fixture / 'Cargo.toml'), '--offline',
                        '--target-dir', str(target)], env=env, stdout=log, stderr=subprocess.STDOUT, check=True)
    with socket.socket() as sock:
        sock.bind(('127.0.0.1', 0))
        port = sock.getsockname()[1]
    (server / 'eula.txt').write_text('eula=true\n')
    (server / 'server.properties').write_text(
        f'server-ip=127.0.0.1\nserver-port={port}\nonline-mode=false\nwhite-list=false\n'
        'level-name=voice-test\nlevel-seed=12345\nview-distance=2\nsimulation-distance=2\n'
        'gamemode=creative\nforce-gamemode=true\nmax-players=3\npause-when-empty-seconds=-1\n')
    processes = []
    threads = []
    logs = []
    def launch(command, cwd, name):
        log = (root / name).open('w')
        logs.append(log)
        process = subprocess.Popen(command, cwd=cwd, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.STDOUT, text=True, bufsize=1)
        processes.append(process)
        messages = queue.Queue()
        def read():
            for line in process.stdout:
                log.write(line)
                log.flush()
                messages.put(line.rstrip())
        thread = threading.Thread(target=read, daemon=True)
        thread.start()
        threads.append(thread)
        return process, messages
    def wait(process, messages, needle, timeout=90):
        deadline = time.monotonic() + timeout
        while time.monotonic() < deadline:
            try:
                line = messages.get(timeout=0.2)
                if needle in line:
                    return line
            except queue.Empty:
                if process.poll() is not None:
                    raise RuntimeError(f'{needle}: process exited {process.returncode}')
        raise TimeoutError(f'no {needle!r}; see {root}')
    def send(process, command):
        process.stdin.write(command + '\n')
        process.stdin.flush()
    result = {'audio_devices_used': False, 'synthetic_pcm_over_real_network': True, 'phases': []}
    try:
        engine, engine_lines = launch([str(args.engine.resolve()), '--resources', str(args.resources.resolve()),
                                       '--dedicated', '--nogui'], server, 'server.log')
        wait(engine, engine_lines, 'Done (')
        client_bin = target / 'debug' / ('voice-client.exe' if os.name == 'nt' else 'voice-client')
        clients, client_lines = launch([str(client_bin), str(port)], root, 'clients.log')
        wait(clients, client_lines, 'READY:')
        phases = [
            (1, ['tp Speaker 0 120 0', 'tp Nearby 3 120 0', 'tp Distant 500 120 0'], ['Nearby']),
            (2, ['tp Nearby 500 120 0', 'tp Distant 6 120 0'], ['Distant']),
            (3, ['execute in minecraft:the_nether run tp Distant 6 120 0'], []),
        ]
        for sequence, commands, expected in phases:
            for command in commands:
                send(engine, command)
            marker = f'VOICE_PHASE_{sequence}_READY'
            send(engine, 'say ' + marker)
            wait(engine, engine_lines, marker)
            time.sleep(0.5)  # Let clients acknowledge the server-authoritative teleports.
            send(clients, f'SEND {sequence}')
            send(clients, f'CHECK {sequence} ' + ' '.join(expected))
            detail = wait(clients, client_lines, f'PASS {sequence}:', timeout=15)
            result['phases'].append({'sequence': sequence, 'receivers': expected, 'evidence': detail})
        # A non-operator must not change the server-wide voice range.
        send(engine, 'tp Nearby 30 120 0')
        send(engine, 'say VOICE_AUTH_READY')
        wait(engine, engine_lines, 'VOICE_AUTH_READY')
        send(clients, 'COMMAND speak distance 8')
        time.sleep(0.5)
        send(clients, 'SEND 4')
        send(clients, 'CHECK 4 Nearby')
        result['phases'].append({'sequence': 4, 'evidence': wait(clients, client_lines, 'PASS 4:', timeout=15)})
        # Operator changes must affect routing immediately, without restarting.
        send(engine, 'op Speaker')
        send(engine, 'say VOICE_OP_READY')
        wait(engine, engine_lines, 'VOICE_OP_READY')
        time.sleep(0.5)
        for sequence, distance, recipients in [(5, 8, []), (6, 40, ['Nearby'])]:
            send(clients, f'COMMAND speak distance {distance}')
            wait(clients, client_lines, f'server voice distance set to {distance} blocks.', timeout=15)
            send(clients, f'SEND {sequence}')
            send(clients, f'CHECK {sequence} ' + ' '.join(recipients))
            result['phases'].append({'sequence': sequence, 'distance': distance,
                                    'evidence': wait(clients, client_lines, f'PASS {sequence}:', timeout=15)})
        send(clients, 'STOP')
        clients.wait(timeout=10)
        if clients.returncode:
            raise RuntimeError(f'client fixture exited {clients.returncode}')
        send(engine, 'stop')
        engine.wait(timeout=60)
        if engine.returncode:
            raise RuntimeError(f'engine exited {engine.returncode}')
        result['passed'] = True
    except Exception as error:
        result['passed'] = False
        result['error'] = str(error)
        raise
    finally:
        for process in reversed(processes):
            if process.poll() is None:
                try:
                    send(process, 'stop' if process is processes[0] else 'STOP')
                    process.wait(timeout=10)
                except (BrokenPipeError, OSError, subprocess.TimeoutExpired):
                    process.kill()
                    process.wait()
        for thread in threads:
            thread.join(timeout=3)
        for log in logs:
            log.close()
        (root / 'result.json').write_text(json.dumps(result, indent=2))
        print(json.dumps(result, indent=2))
        print('Evidence:', root)

if __name__ == '__main__':
    main()
