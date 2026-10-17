#!/usr/bin/env python3
"""Bounded live nested IPC, process ownership and socket-collision regression checks."""
import argparse
import json
import os
from pathlib import Path
import signal
import socket
import struct
import subprocess
import sys
import time
import uuid

from smoke_nested import require, sha256, write_json


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--artifacts', type=Path)
    args = parser.parse_args()
    binary = args.binary.resolve()
    parent = dict(os.environ)
    runtime = Path(parent['XDG_RUNTIME_DIR'])
    artifacts = (args.artifacts or root / 'target/smoke-ipc' / uuid.uuid4().hex).resolve()
    artifacts.mkdir(parents=True, exist_ok=False)
    deadline = time.monotonic() + 25
    name = 'fluxwm-ipc-' + uuid.uuid4().hex[:16]
    ready, stats = artifacts / 'ready.json', artifacts / 'stats.json'
    result = {'ok': False, 'binary_sha256': sha256(binary), 'responses': [], 'scope': 'nested IPC and direct application ownership; no native/session/descendant qualification'}
    process = None
    child_pid = None
    child_pidfd = None
    def check_time():
        require(time.monotonic() < deadline, 'suite deadline exceeded')
    def wait_for(test):
        while not test():
            check_time()
            require(process is None or process.poll() is None, 'compositor exited early')
            time.sleep(.01)
    def request(method, params=None, version=1, ident=None):
        return {'version': version, 'id': ident or uuid.uuid4().hex, 'method': method, 'params': params or {}}
    def frame(value):
        data = json.dumps(value, separators=(',', ':')).encode()
        return struct.pack('>I', len(data)) + data
    def receive(connection):
        def exact(count):
            data = bytearray()
            while len(data) < count:
                check_time()
                chunk = connection.recv(count - len(data))
                require(bool(chunk), 'unexpected IPC EOF')
                data.extend(chunk)
            return bytes(data)
        length, = struct.unpack('>I', exact(4))
        require(0 < length <= 1024 * 1024, 'response length outside limit')
        value = json.loads(exact(length))
        result['responses'].append(value)
        return value
    def exchange(messages, half_close=False):
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(2)
            connection.connect(str(ipc))
            connection.sendall(b''.join(map(frame, messages)))
            if half_close:
                connection.shutdown(socket.SHUT_WR)
            replies = [receive(connection) for _ in messages]
            require([v['id'] for v in replies] == [v['id'] for v in messages], 'reply identity/order mismatch')
            return replies
    try:
        # Check collision preservation before backend initialization. Both a
        # regular endpoint and a dangling lock symlink must survive unchanged.
        for suffix in ('', '.lock'):
            collision = runtime / (name + suffix)
            if suffix:
                collision.symlink_to('fluxwm-missing-' + uuid.uuid4().hex)
            else:
                collision.write_bytes(b'owned regression sentinel\n')
            before = collision.lstat()
            try:
                completed = subprocess.run([str(binary), '--no-config', '--socket', name, '--exit-after-ms', '100'], stdout=subprocess.PIPE, stderr=subprocess.PIPE, timeout=3)
                after = collision.lstat()
                require(completed.returncode != 0, 'existing socket/lock was accepted')
                require((before.st_dev, before.st_ino) == (after.st_dev, after.st_ino), 'collision inode changed')
                if not suffix:
                    require(collision.read_bytes() == b'owned regression sentinel\n', 'collision contents changed')
                (artifacts / ('collision-lock.log' if suffix else 'collision-path.log')).write_bytes(completed.stderr)
            finally:
                # This name is random and was created by this harness.
                if collision.lstat().st_ino == before.st_ino:
                    collision.unlink()
        with (artifacts / 'stdout.log').open('wb') as out, (artifacts / 'stderr.log').open('wb') as err:
            process = subprocess.Popen([str(binary), '--no-config', '--socket', name, '--ready-file', str(ready), '--stats-file', str(stats), '--exit-after-ms', '20000'], stdout=out, stderr=err, env=parent, start_new_session=True)
            wait_for(ready.exists)
            published = json.loads(ready.read_text())
            require(published['pid'] == process.pid, 'readiness PID mismatch')
            ipc = Path(published['ipc_socket'])
            replies = exchange([request('version', ident=str(i)) for i in range(17)], half_close=True)
            require(all('result' in v for v in replies), '17-request half-close pipeline failed')
            before, changed, invalid = exchange([request('version'), request('action', {'name': 'workspace-mode', 'mode': 'canvas'}), request('version', version=99)])
            require(int(changed['state_sequence']) > int(before['state_sequence']), 'action did not advance sequence')
            require('result' in changed and invalid.get('error', {}).get('code') == 'unsupported_version', 'sequence test envelopes failed')
            require(changed['state_sequence'] == invalid['state_sequence'], 'transport error has stale state sequence')
            bad, capability, locked = exchange([request('action', {'name': 'quit', 'unexpected': True}), request('capabilities'), request('action', {'name': 'lock'})])
            require(bad.get('error', {}).get('code') == 'invalid_params', 'unknown action field accepted')
            require(locked.get('error', {}).get('code') == 'not_ready', 'unimplemented lock accepted')
            require(all(capability['result'][k] is False for k in ('native_session', 'capture', 'secure_lock', 'x11_clients', 'drag_and_drop')), 'unsupported capability advertised')
            # Missing executable completes asynchronously without killing worker.
            missing = exchange([request('action', {'name': 'spawn', 'argv': ['/fluxwm-missing-' + uuid.uuid4().hex]})])[0]
            require('result' in missing, 'launch queue did not accept missing executable job')
            def missing_completed():
                response = exchange([request('diagnostics')])[0]['result']
                matches = [outcome for outcome in response['launch_outcomes'] if outcome['operation_id'] == missing['result']['operation_id']]
                return response['pending_launches'] == 0 and len(matches) == 1 and matches[0]['error'] is not None and matches[0]['pid'] is None
            wait_for(missing_completed)
            marker = artifacts / 'child.json'
            code = 'import json,os,signal,sys,time; m=signal.pthread_sigmask(signal.SIG_BLOCK,[]); tmp=sys.argv[1]+".tmp"; open(tmp,"w").write(json.dumps({"pid":os.getpid(),"display":os.environ.get("WAYLAND_DISPLAY"),"ipc":os.environ.get("FLUXWM_SOCKET"),"host_display":os.environ.get("DISPLAY"),"wayland_socket":os.environ.get("WAYLAND_SOCKET"),"blocked":[int(x) for x in m]})); os.replace(tmp,sys.argv[1]); time.sleep(30)'
            launched = exchange([request('action', {'name': 'spawn', 'argv': [sys.executable, '-c', code, str(marker)]})])[0]
            require('result' in launched, 'recovery launch not accepted')
            wait_for(lambda: marker.exists() and marker.stat().st_size > 0)
            child = json.loads(marker.read_text())
            child_pid = child['pid']
            child_pidfd = os.pidfd_open(child_pid)
            require(child['display'] == name and child['ipc'] == str(ipc), 'child instance environment mismatch')
            require(child['host_display'] is None and child['wayland_socket'] is None, 'child inherited host display route')
            require(not set(child['blocked']) & {int(signal.SIGINT), int(signal.SIGTERM), int(signal.SIGCHLD)}, 'child inherited compositor signal masks')
            diagnostics = exchange([request('diagnostics')])[0]['result']
            require(diagnostics['owned_applications'] == 1 and diagnostics['pending_launches'] == 0, 'live process ownership missing')
            require(any(o['operation_id'] == launched['result']['operation_id'] and o['pid'] == child_pid and o['error'] is None for o in diagnostics['launch_outcomes']), 'successful launch outcome not correlated')
            require('result' in exchange([request('action', {'name': 'quit'})])[0], 'quit rejected')
            process.wait(timeout=4)
        require(process.returncode == 0, f'compositor failed: {process.returncode}')
        try:
            os.kill(child_pid, 0)
        except ProcessLookupError:
            result['child_absent_after_shutdown'] = True
        else:
            raise RuntimeError('supervised child remains after compositor shutdown')
        require(not any(p.exists() for p in (runtime / name, runtime / (name + '.lock'), ipc)), 'owned socket leaked')
        require(dict(os.environ) == parent, 'parent environment changed')
        result['binary_sha256_after'] = sha256(binary)
        require(result['binary_sha256_after'] == result['binary_sha256'], 'binary changed during test')
        result['stats'] = json.loads(stats.read_text())
        result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
            process.wait(timeout=2)
            result['forced_cleanup'] = True
        if child_pidfd is not None:
            if not result['ok']:
                try:
                    signal.pidfd_send_signal(child_pidfd, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            os.close(child_pidfd)
        write_json(artifacts / 'result.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(artifacts), 'error': result.get('error')}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    sys.exit(main())
