#!/usr/bin/env python3
"""Bounded real nested quit pipeline and unread-peer shutdown regression."""
import argparse
import json
import os
from pathlib import Path
import socket
import struct
import subprocess
import time
import uuid

from smoke_nested import require, sha256, write_json


def frame(ident, method, params=None):
    raw = json.dumps({'version': 1, 'id': ident, 'method': method, 'params': params or {}}, separators=(',', ':')).encode()
    return struct.pack('>I', len(raw)) + raw


def run_case(binary, directory, case, environment):
    directory.mkdir(mode=0o700)
    name = 'fluxwm-shutdown-' + uuid.uuid4().hex[:12]
    ready, stats = directory / 'ready.json', directory / 'stats.json'
    result = {'ok': False, 'case': case, 'binary_sha256': sha256(binary)}
    process = None
    sockets = []
    started = time.monotonic()
    deadline = started + 12
    paths = []

    def check():
        require(time.monotonic() < deadline, 'case deadline exceeded')
        for name in ['stdout.log', 'stderr.log']:
            require((directory / name).stat().st_size <= 4 * 1024 * 1024, 'log budget exceeded')

    def connect(path):
        connection = socket.socket(socket.AF_UNIX)
        sockets.append(connection)
        connection.settimeout(2)
        connection.connect(str(path))
        return connection

    try:
        with (directory / 'stdout.log').open('wb') as out, (directory / 'stderr.log').open('wb') as err:
            command = [str(binary), '--socket', name, '--ready-file', str(ready), '--stats-file', str(stats), '--exit-after-ms', '10000']
            write_json(directory / 'command.json', {'argv': command, 'host_environment_saved': False})
            process = subprocess.Popen(command, env=environment, stdout=out, stderr=err, start_new_session=True)
            while not ready.exists():
                check()
                require(process.poll() is None, 'compositor exited before readiness')
                time.sleep(.01)
            publication = json.loads(ready.read_text())
            require(publication['pid'] == process.pid and publication['wayland_display'] == name, 'instance readiness mismatch')
            ipc = Path(publication['ipc_socket'])
            require(ipc == Path(environment['XDG_RUNTIME_DIR']) / 'fluxwm' / (name + '.sock'), 'IPC path identity mismatch')
            for path in [ipc, Path(environment['XDG_RUNTIME_DIR']) / name, Path(environment['XDG_RUNTIME_DIR']) / (name + '.lock')]:
                metadata = path.lstat()
                paths.append((path, metadata.st_dev, metadata.st_ino))
            if case == 'slow-peer':
                slow = connect(ipc)
                slow.setblocking(False)
                payload = b''.join(frame('unread-' + str(i), 'diagnostics') for i in range(4096))
                sent = 0
                send_until = time.monotonic() + .4
                while sent < len(payload) and time.monotonic() < send_until:
                    check()
                    try:
                        sent += slow.send(payload[sent:])
                    except BlockingIOError:
                        time.sleep(.005)
                require(sent >= 65536, 'insufficient slow-peer input admitted for saturation prerequisite')
                result['unread_peer_request_bytes_sent'] = sent
                # No receives on slow: the final drain timeout counter proves
                # a pending frame survived to the absolute deadline.
                time.sleep(.2)
            controller = connect(ipc)
            controller.sendall(frame('before', 'version') + frame('quit', 'action', {'name': 'quit'}) + frame('tail', 'version'))
            quit_sent = time.monotonic()
            captured = bytearray()
            while True:
                check()
                try:
                    data = controller.recv(8192)
                except ConnectionResetError:
                    result['read_reset_after_frames'] = True
                    break
                if not data:
                    break
                captured.extend(data)
                require(len(captured) <= 1024 * 1024, 'reply budget exceeded')
            result['controller_eof_ms'] = round((time.monotonic() - quit_sent) * 1000, 2)
            responses = []
            offset = 0
            while offset < len(captured):
                require(len(captured) - offset >= 4, 'truncated reply prefix')
                length, = struct.unpack_from('>I', captured, offset)
                offset += 4
                require(0 < length <= 1024 * 1024 and len(captured) - offset >= length, 'truncated or invalid reply')
                responses.append(json.loads(captured[offset:offset + length]))
                offset += length
            result['responses'] = responses
            # Collect natural shutdown and cleanup evidence even if the response
            # oracle fails; do not turn an observed defect into harness SIGTERM.
            process.wait(timeout=min(3, max(.001, deadline - time.monotonic())))
            result['process_exit_ms'] = round((time.monotonic() - quit_sent) * 1000, 2)
            result['stats'] = json.loads(stats.read_text())
            require([r['id'] for r in responses] == ['before', 'quit'], 'pipeline after quit executed or acknowledgement missing')
            require(all('result' in r for r in responses), 'quit pipeline rejected')
            require(int(responses[1]['state_sequence']) == int(responses[0]['state_sequence']) + 1, 'quit sequence mismatch')
            require(process.returncode == 0, 'compositor failed during shutdown')
            require(result['process_exit_ms'] < 2000, 'shutdown exceeded local two-second test tolerance')
            timed_out = result['stats'].get('ipc_drain_timed_out', 0)
            require(timed_out >= 1 if case == 'slow-peer' else timed_out == 0, 'pending-frame deadline oracle failed')
            require(all(not path.exists() and not path.is_symlink() for path, _, _ in paths), 'owned endpoint survived shutdown')
            result['endpoints_removed'] = True
            result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
    finally:
        for connection in sockets:
            connection.close()
        if process is not None and process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=2)
        result['exit_code'] = process.returncode if process else None
        result['endpoints_removed_after_cleanup'] = all(not p.exists() and not p.is_symlink() for p, _, _ in paths)
        result['elapsed_seconds'] = round(time.monotonic() - started, 3)
        result['binary_unchanged'] = result['binary_sha256'] == sha256(binary)
        require(result['binary_unchanged'], 'binary changed during evidence run')
        write_json(directory / 'result.json', result)
    return result


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--case', choices=['all', 'pipeline', 'slow-peer'], default='all')
    args = parser.parse_args()
    binary = args.binary.resolve()
    directory = (args.artifacts or root / 'target/smoke-shutdown' / uuid.uuid4().hex).resolve()
    directory.mkdir(parents=True, mode=0o700, exist_ok=False)
    parent = dict(os.environ)
    cases = ['pipeline', 'slow-peer'] if args.case == 'all' else [args.case]
    results = [run_case(binary, directory / case, case, parent) for case in cases]
    summary = {'ok': all(r['ok'] for r in results), 'cases': results, 'parent_environment_unchanged': parent == dict(os.environ), 'harness_sha256': sha256(Path(__file__)), 'scope': 'nested IPC quit response, pipeline admission and unread-peer deadline; no native or universal teardown bound'}
    write_json(directory / 'summary.json', summary)
    print(json.dumps({'ok': summary['ok'], 'artifacts': str(directory)}))
    return 0 if summary['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
