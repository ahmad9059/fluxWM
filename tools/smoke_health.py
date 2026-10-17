#!/usr/bin/env python3
"""Real XDG ping/pong, late recovery, binding ownership and monitor-cap tests."""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import sys
import time
import uuid

from smoke_nested import require, sha256, write_json


def run_case(binary, fixture, directory, mode, parent):
    directory.mkdir(mode=0o700)
    instance = 'fluxhealth-' + uuid.uuid4().hex[:16]
    ready, stats = directory / 'ready.json', directory / 'stats.json'
    result = {'ok': False, 'mode': mode, 'binary_sha256': sha256(binary), 'fixture_sha256': sha256(fixture)}
    process = client = None
    streams = []
    deadline = time.monotonic() + 20
    start = time.monotonic()
    endpoints = []

    def check():
        require(time.monotonic() < deadline, 'health case deadline exceeded')
        for path in directory.glob('*.log'):
            require(path.stat().st_size < 4 * 1024 * 1024, 'log budget exceeded')

    def ipc(method, params=None):
        ident = uuid.uuid4().hex
        payload = json.dumps({'version': 1, 'id': ident, 'method': method, 'params': params or {}}).encode()
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(min(2, max(.001, deadline - time.monotonic())))
            connection.connect(str(ipc_path))
            connection.sendall(struct.pack('>I', len(payload)) + payload)
            def exact(count):
                raw = b''
                while len(raw) < count:
                    check()
                    part = connection.recv(count - len(raw))
                    require(part, 'IPC EOF')
                    raw += part
                return raw
            size, = struct.unpack('>I', exact(4))
            require(0 < size <= 1024 * 1024, 'IPC reply bound')
            reply = json.loads(exact(size))
            require(reply['id'] == ident and 'result' in reply, 'IPC operation failed')
            return reply['result']
    try:
        for name in ['compositor.stdout.log', 'compositor.stderr.log', 'fixture.stdout.log', 'fixture.stderr.log']:
            streams.append((directory / name).open('wb'))
        command = [str(binary), '--socket', instance, '--ready-file', str(ready), '--stats-file', str(stats), '--exit-after-ms', '18000']
        write_json(directory / 'command.json', {'compositor': command, 'fixture': [str(fixture), mode, '--timeout-ms', '12000']})
        process = subprocess.Popen(command, env=parent, stdout=streams[0], stderr=streams[1], start_new_session=True)
        while not ready.exists():
            check(); require(process.poll() is None, 'compositor exited before ready'); time.sleep(.01)
        publication = json.loads(ready.read_text())
        require(publication['pid'] == process.pid and publication['wayland_display'] == instance, 'readiness identity mismatch')
        ipc_path = Path(publication['ipc_socket'])
        require(ipc_path == Path(parent['XDG_RUNTIME_DIR']) / 'fluxwm' / (instance + '.sock'), 'IPC instance path mismatch')
        endpoints = [Path(parent['XDG_RUNTIME_DIR']) / instance, Path(parent['XDG_RUNTIME_DIR']) / (instance + '.lock'), ipc_path]
        before = ipc('diagnostics')
        require(before['shell_health']['clients'] == [] and before['shell_health']['timer_armed'] is False, 'empty monitor not idle')
        child_environment = dict(parent, WAYLAND_DISPLAY=instance, FLUXWM_SOCKET=str(ipc_path))
        child_environment.pop('WAYLAND_SOCKET', None)
        client = subprocess.Popen([str(fixture), mode, '--timeout-ms', '12000'], env=child_environment, stdout=streams[2], stderr=streams[3], start_new_session=True)
        while client.poll() is None:
            check(); require(process.poll() is None, 'compositor died while client tested health'); time.sleep(.01)
        require((directory / 'fixture.stdout.log').stat().st_size <= 1024 * 1024, 'fixture JSON bound exceeded')
        report = json.loads((directory / 'fixture.stdout.log').read_text())
        result['fixture_report'] = report
        require(client.returncode == 0 and report['ok'], 'real fixture oracle failed: ' + str(report.get('error')))
        cleanup_start = time.monotonic()
        after = ipc('diagnostics')
        while after['shell_health']['clients'] and time.monotonic() - cleanup_start < .5:
            check(); time.sleep(.01); after = ipc('diagnostics')
        result['disconnect_cleanup_ms'] = round((time.monotonic() - cleanup_start) * 1000, 2)
        require(result['disconnect_cleanup_ms'] <= 500, 'disconnect observation exceeded 500 ms including IPC wait')
        require(after['shell_health']['clients'] == [] and after['shell_health']['timer_armed'] is False, 'monitor/timer survived client retirement')
        result['after_disconnect'] = after
        result['quit'] = ipc('action', {'name': 'quit'})
        process.wait(timeout=min(2, max(.001, deadline - time.monotonic())))
        require(process.returncode == 0, 'compositor normal exit failed')
        require(all(not p.exists() and not p.is_symlink() for p in endpoints), 'owned endpoints survived')
        result['stats'] = json.loads(stats.read_text())
        result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
    finally:
        for child in [client, process]:
            if child is not None and child.poll() is None:
                child.terminate()
                try: child.wait(timeout=2)
                except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=2)
        for stream in streams: stream.close()
        result['compositor_exit'] = process.returncode if process else None
        result['fixture_exit'] = client.returncode if client else None
        result['elapsed_seconds'] = round(time.monotonic() - start, 3)
        result['endpoints_removed'] = all(not p.exists() and not p.is_symlink() for p in endpoints)
        result['executables_unchanged'] = result['binary_sha256'] == sha256(binary) and result['fixture_sha256'] == sha256(fixture)
        result['ok'] &= result['executables_unchanged']
        write_json(directory / 'result.json', result)
    return result


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--fixture', type=Path, default=root / 'target/debug/fluxwm-fixture')
    parser.add_argument('--artifacts', type=Path)
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-health' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    parent = dict(os.environ)
    cases = [run_case(args.binary.resolve(), args.fixture.resolve(), directory / mode, mode, parent)
             for mode in ['shell-health', 'shell-health-capacity', 'shell-health-disconnect']]
    result = {'ok': all(case['ok'] for case in cases), 'cases': cases, 'harness_sha256': sha256(Path(__file__)),
              'parent_environment_unchanged': parent == dict(os.environ), 'scope': 'synthetic XDG shell resource responsiveness, nonfatal diagnostics and monitor bounds; no application/frame-progress/native or global resource qualification'}
    write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
