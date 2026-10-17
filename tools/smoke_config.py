#!/usr/bin/env python3
"""Runtime Lua publication and failure retention on an owned private Xvfb."""
from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import re
import socket
import struct
import subprocess
import time
import uuid

from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import require, sha256, write_json


def generation(value):
    require(isinstance(value, str) and re.fullmatch(r'0|[1-9][0-9]*', value),
            'generation must be a canonical decimal string')
    number = int(value)
    require(number <= 2**64 - 1, 'generation exceeds u64')
    return number


def lua(gap, mode, marker=None, unknown=False, route=False):
    # JSON string quoting is also valid for these ASCII Lua paths/chords.
    bindings = [('Super+Shift+q', 'quit'), ('Super+r', 'reload-config'), ('Super+q', 'close-window')]
    if unknown:
        bindings.append(('Super+FluxwmDefinitelyUnknownKeysym', 'quit'))
    entries = ',\n'.join('{ key = { sym = ' + json.dumps(key) + ' }, action = { name = ' + json.dumps(action) + ' } }'
                         for key, action in bindings)
    if route:
        entries += ', { key = { sym = "Alt+a" }, action = { name = "workspace-mode", mode = "canvas" } }'
    autostart = '' if marker is None else ', autostart = { { argv = { "/usr/bin/touch", ' + json.dumps(str(marker)) + ' } } }'
    return f'return {{ schema_version = 1, layout = {{ gaps = {gap}, mode = "{mode}" }}, bindings = {{ {entries} }}{autostart} }}\n'


class Case:
    def __init__(self, binary, directory, environment, config_text, disabled=False):
        self.binary, self.directory, self.environment = binary, directory, environment
        directory.mkdir(mode=0o700)
        self.deadline = time.monotonic() + 18
        self.process = None
        self.streams = []
        self.trace = []
        self.trace_bytes = 0
        self.result = {'ok': False}
        self.instance = 'fluxconfig-' + uuid.uuid4().hex[:12]
        self.config = directory / 'config.lua'
        self.marker = directory / 'forbidden-autostart'
        self.ready = directory / 'ready.json'
        self.stats = directory / 'stats.json'
        self.endpoints = [Path(environment['XDG_RUNTIME_DIR']) / self.instance,
                          Path(environment['XDG_RUNTIME_DIR']) / (self.instance + '.lock'),
                          Path(environment['XDG_RUNTIME_DIR']) / 'fluxwm' / (self.instance + '.sock')]
        self.ipc_path = self.endpoints[-1]
        if config_text is not None:
            self.replace(config_text)
        self.command = [str(binary), '--socket', self.instance, '--ready-file', str(self.ready),
                        '--stats-file', str(self.stats), '--exit-after-ms', '17000']
        self.command += ['--no-config'] if disabled else ['--config', str(self.config)]

    def check(self):
        require(time.monotonic() < self.deadline, 'configuration case deadline exceeded')
        require(self.process is None or self.process.poll() is None, 'compositor exited before case completed')
        require(not self.marker.exists(), 'rejected configuration executed autostart')
        for name in ['stdout.log', 'stderr.log']:
            path = self.directory / name
            if path.exists():
                require(path.stat().st_size <= 4 * 1024 * 1024, 'configuration log budget exceeded')

    def replace(self, text):
        staging = self.config.with_suffix('.next')
        staging.write_text(text)
        staging.replace(self.config)
        # Keep each attempted source, including invalid Lua, as public evidence.
        (self.directory / f'config-attempt-{len(list(self.directory.glob("config-attempt-*")))}.lua').write_text(text)

    def wait(self, predicate, maximum=5):
        until = min(self.deadline, time.monotonic() + maximum)
        while True:
            self.check()
            value = predicate()
            if value:
                return value
            require(time.monotonic() < until, 'configuration observation deadline exceeded')
            time.sleep(.01)

    def start(self):
        write_json(self.directory / 'command.json', {'compositor': self.command})
        self.streams = [(self.directory / name).open('wb') for name in ['stdout.log', 'stderr.log']]
        self.process = subprocess.Popen(self.command, env=self.environment, stdout=self.streams[0],
                                        stderr=self.streams[1], start_new_session=True)
        self.wait(self.ready.exists, 8)
        ready = json.loads(self.ready.read_text())
        require(ready['pid'] == self.process.pid and ready['wayland_display'] == self.instance and
                Path(ready['ipc_socket']) == self.ipc_path, 'readiness identity mismatch')

    def ipc(self, method, params=None, error=False):
        self.check()
        ident = uuid.uuid4().hex
        request = {'version': 1, 'id': ident, 'method': method, 'params': params or {}}
        payload = json.dumps(request).encode()
        with socket.socket(socket.AF_UNIX) as stream:
            def arm():
                remaining = self.deadline - time.monotonic()
                require(remaining > 0, 'IPC absolute deadline exceeded')
                stream.settimeout(min(2, remaining))
            arm(); stream.connect(str(self.ipc_path)); arm(); stream.sendall(struct.pack('>I', len(payload)) + payload)
            def exact(count):
                data = bytearray()
                while len(data) < count:
                    self.check(); arm()
                    part = stream.recv(count - len(data))
                    require(part, 'IPC EOF before complete response')
                    data.extend(part)
                return data
            length, = struct.unpack('>I', exact(4))
            require(0 < length <= 1024 * 1024, 'configuration IPC response bound exceeded')
            response = json.loads(exact(length))
        self.trace_bytes += len(payload) + length
        require(self.trace_bytes <= 16 * 1024 * 1024, 'IPC trace byte bound exceeded')
        self.trace.append({'request': request, 'response': response})
        require(len(self.trace) <= 1200, 'IPC trace record bound exceeded')
        require(response['id'] == ident and response['version'] == 1, 'IPC response identity/version mismatch')
        require(('error' in response) if error else ('result' in response), 'unexpected IPC result: ' + str(response))
        return response.get('error') if error else response['result']

    def configuration(self):
        diagnostic = self.ipc('diagnostics')
        config = diagnostic['configuration']
        for key in ('requested_generation', 'applied_generation'):
            if config[key] is not None:
                generation(config[key])
        require(config['status'] in ('pending', 'applied', 'failed', 'disabled'), 'unknown configuration status')
        return diagnostic

    def await_status(self, status, requested=None):
        def ready():
            diagnostic = self.configuration()
            config = diagnostic['configuration']
            return diagnostic if config['status'] == status and (requested is None or config['requested_generation'] == requested) else None
        return self.wait(ready)

    def reload(self):
        before = self.configuration()['configuration']['requested_generation']
        accepted = self.ipc('config.reload')
        require(accepted['accepted'] is True, 'reload not accepted')
        require(generation(accepted['generation']) > (generation(before) if before is not None else 0), 'reload generation did not advance')
        return accepted['generation']

    def snapshot(self, label):
        diagnostic = self.configuration()
        effective = self.ipc('config.effective')
        workspaces = self.ipc('workspaces.list')
        value = {'diagnostics': diagnostic, 'effective': effective, 'workspaces': workspaces}
        write_json(self.directory / (label + '.json'), value)
        return value

    def close(self):
        if self.process is not None and self.process.poll() is None:
            self.process.terminate()
            try:
                self.process.wait(timeout=2)
            except subprocess.TimeoutExpired:
                self.process.kill(); self.process.wait(timeout=2)
        for stream in self.streams:
            stream.close()
        self.result['exit_code'] = self.process.returncode if self.process else None
        self.result['endpoints_removed'] = all(not path.exists() and not path.is_symlink() for path in self.endpoints)
        self.result['forbidden_autostart_absent'] = not self.marker.exists()
        self.result['ok'] &= self.result['endpoints_removed'] and self.result['forbidden_autostart_absent'] and self.result['exit_code'] == 0
        write_json(self.directory / 'ipc-trace.json', self.trace)
        write_json(self.directory / 'result.json', self.result)


def assert_applied(snapshot, expected_generation, gap, mode, entries=3):
    config = snapshot['diagnostics']['configuration']
    require(config['status'] == 'applied' and config['applied_generation'] == expected_generation and config['requested_generation'] == expected_generation and config['last_error'] is None,
            'applied generation/status mismatch')
    bindings = snapshot['diagnostics']['keyboard_bindings']
    require(bindings['source'] == 'user-config' and bindings['entries'] == entries and bindings['inactive'] == 0,
            'user binding publication differs')
    require(snapshot['workspaces']['workspaces'][0]['mode'] == mode, 'live workspace mode differs')
    # Exact effective shape is intentionally strict; update only with runtime API agreement.
    effective = snapshot['effective']
    require(effective['generation'] == expected_generation, 'effective generation differs')
    require(effective['desired'] == {'layout': {'gaps': gap, 'mode': mode}, 'binding_entries': entries}, 'desired configuration differs')
    require(effective['effective']['binding_entries'] == entries and effective['effective']['keymap'] == 'startup-seat-keymap', 'effective binding/keymap differs')
    require(set(effective['unsupported']) == {'layout.outer_gap', 'layout.width_presets', 'layout.default_width', 'layout.focus_margin', 'layout.canvas', 'input', 'outputs', 'animations', 'decoration', 'rules', 'autostart', 'security'}, 'unsupported setting categories differ')
    require(effective['effective']['layout']['gaps'] == gap and effective['effective']['layout']['mode'] == mode,
            'effective layout differs')


def exercise(binary, directory, environment, name):
    case = Case(binary, directory / name, environment, None, disabled=name == 'disabled')
    try:
        if name == 'invalid-startup':
            case.replace(lua(23, 'canvas', case.marker, unknown=True))
        elif name == 'reload-retention':
            case.replace(lua(17, 'canvas'))
        case.start()
        if name == 'disabled':
            initial = case.await_status('disabled')
            require(initial['keyboard_bindings']['source'] == 'development-defaults', 'no-config replaced defaults')
            require(initial['configuration']['requested_generation'] is None and initial['configuration']['applied_generation'] is None, 'disabled configuration allocated a generation')
            require(case.ipc('capabilities')['config_reload'] is False, 'disabled instance advertises reload')
            case.result['reload_rejected'] = case.ipc('config.reload', error=True)
            require(case.result['reload_rejected']['code'] == 'not_ready', 'disabled reload has wrong error code')
            case.result['snapshot'] = case.snapshot('disabled')
        elif name == 'invalid-startup':
            initial = case.await_status('failed')
            require(initial['keyboard_bindings']['source'] == 'development-defaults', 'failed startup replaced defaults')
            require(initial['configuration']['applied_generation'] is None, 'failed startup published a generation')
            require(initial['configuration']['last_error'], 'failed startup lacks an error')
            case.result['snapshot'] = case.snapshot('failed-startup')
            fallback = case.result['snapshot']['effective']
            require(fallback['generation'] is None and fallback['desired'] is None, 'failed startup published desired config')
            require(fallback['effective']['layout'] == {'gaps': 8.0, 'mode': 'strip'}, 'failed startup changed default layout')
        else:
            initial = case.await_status('applied')
            first = initial['configuration']['applied_generation']
            assert_applied(case.snapshot('startup-applied'), first, 17, 'canvas')
            case.replace(lua(29, 'strip'))
            applied = case.reload(); case.await_status('applied', applied)
            good = case.snapshot('reload-applied'); assert_applied(good, applied, 29, 'strip')
            for label, source in [('invalid-lua', 'return { layout = { gaps = this is invalid Lua } }\n'),
                                  ('unknown-symbol', lua(41, 'canvas', case.marker, unknown=True))]:
                case.replace(source)
                rejected = case.reload()
                failed = case.await_status('failed', rejected)
                require(failed['configuration']['applied_generation'] == applied, 'failure replaced applied generation')
                require(failed['configuration']['last_error'], 'failed reload lacks an error')
                retained = case.snapshot(label)
                require(retained['effective']['effective'] == good['effective']['effective'], 'failure changed effective settings')
                require(retained['workspaces'] == good['workspaces'], 'failure changed workspaces')
                require(retained['diagnostics']['keyboard_bindings'] == good['diagnostics']['keyboard_bindings'], 'failure changed bindings')
            case.replace(lua(11, 'canvas'))
            recovered = case.reload(); case.await_status('applied', recovered)
            assert_applied(case.snapshot('recovery-applied'), recovered, 11, 'canvas')
            case.result['successful_generations'] = [first, applied, recovered]
        # Observe an additional bounded interval for forbidden launch side effects.
        stop = time.monotonic() + .25
        while time.monotonic() < stop:
            case.check(); time.sleep(.01)
        case.result['ok'] = True
    except Exception as error:
        case.result['error'] = str(error)
    finally:
        case.close()
    return case.result


def extra_case(binary, fixture, directory, environment, session, name):
    case = Case(binary, directory / name, environment, None)
    client = None
    fixture_streams = []
    journal_path = case.directory / 'input.events.jsonl'
    def events():
        if not journal_path.exists():
            return []
        require(journal_path.stat().st_size <= 1024 * 1024, 'input journal exceeds bound')
        text = journal_path.read_text()
        lines = text.splitlines()
        if text and not text.endswith('\n'):
            lines = lines[:-1]
        require(len(lines) <= 1024, 'input journal event bound exceeded')
        return [record.get('event', {}) for record in map(json.loads, lines)]
    def seen(kind, since=0, **values):
        return any(event.get('type') == kind and all(event.get(key) == value for key, value in values.items())
                   for event in events()[since:])
    def inject(kind, **values):
        case.check()
        return session.test(case.process.pid, {'kind': kind, **values})
    try:
        case.replace(lua(13, 'strip', route=name == 'input-publication'))
        case.start()
        initial = case.await_status('applied')['configuration']['applied_generation']
        good = case.snapshot('initial')
        assert_applied(good, initial, 13, 'strip', 4 if name == 'input-publication' else 3)
        if name == 'path-retention':
            for label in ['missing-path', 'layout-gap-out-of-range']:
                if label == 'missing-path':
                    case.config.unlink()
                else:
                    case.replace(lua(2048, 'canvas', case.marker))
                requested = case.reload()
                failed = case.await_status('failed', requested)
                require(failed['configuration']['applied_generation'] == initial and failed['configuration']['last_error'], 'path/layout failure replaced applied config')
                snapshot = case.snapshot(label)
                require(snapshot['effective']['effective'] == good['effective']['effective'], 'path/layout failure changed effective config')
                require(snapshot['workspaces'] == good['workspaces'], 'path/layout failure changed workspace')
                require(snapshot['diagnostics']['keyboard_bindings'] == good['diagnostics']['keyboard_bindings'], 'path/layout failure changed bindings')
            case.replace(lua(19, 'canvas'))
            requested = case.reload(); case.await_status('applied', requested)
            assert_applied(case.snapshot('path-recovered'), requested, 19, 'canvas')
        else:
            case.result['target'] = session.test(case.process.pid)
            inject('focus', focused=True)
            fixture_environment = dict(environment, WAYLAND_DISPLAY=case.instance, FLUXWM_SOCKET=str(case.ipc_path), FLUXWM_FIXTURE_EVENT_FILE=str(journal_path))
            fixture_environment.pop('FLUXWM_FIXTURE_GRAB_POPUP', None)
            fixture_streams = [(case.directory / name).open('wb') for name in ['fixture.stdout.log', 'fixture.stderr.log']]
            client = subprocess.Popen([str(fixture), 'input-probe', '--timeout-ms', '10000'], env=fixture_environment,
                                      stdout=fixture_streams[0], stderr=fixture_streams[1], start_new_session=True)
            case.wait(lambda: seen('ready'), 8)
            case.wait(lambda: seen('keyboard.enter'))
            inject('key', key='Alt_L', pressed=True)
            case.wait(lambda: seen('keyboard.key', key=56, state='pressed'))
            case.wait(lambda: case.ipc('diagnostics')['input_state']['pressed_keys'] == 1)
            case.replace(lua(31, 'strip', route=True))
            requested = case.reload()
            pending_start = time.monotonic()
            samples = []
            while time.monotonic() - pending_start < .35:
                snapshot = case.snapshot('held-pending')
                config = snapshot['diagnostics']['configuration']
                require(config['status'] == 'pending' and config['requested_generation'] == requested and config['applied_generation'] == initial, 'held key allowed configuration publication')
                require(snapshot['effective']['effective'] == good['effective']['effective'], 'held key changed effective config')
                require(snapshot['diagnostics']['input_state']['pressed_keys'] == 1, 'held Alt disappeared during reload')
                samples.append(config)
                time.sleep(.04)
            case.result['held_pending_samples'] = samples
            case.result['held_pending_ms'] = round((time.monotonic() - pending_start) * 1000, 2)
            inject('key', key='Alt_L', pressed=False)
            case.wait(lambda: seen('keyboard.key', key=56, state='released'))
            case.await_status('applied', requested)
            assert_applied(case.snapshot('after-release-applied'), requested, 31, 'strip', 4)
            # Execute a shortcut from the newly published user table. A is absent
            # from the three control bindings and no IPC action changes mode here.
            chord = len(events())
            inject('key', key='Alt_L', pressed=True)
            inject('key', key='a', pressed=True)
            case.wait(lambda: case.ipc('workspaces.list')['workspaces'][0]['mode'] == 'canvas')
            case.wait(lambda: case.ipc('diagnostics')['input_state']['suppressed_keys'] == 1)
            inject('key', key='a', pressed=False); inject('key', key='Alt_L', pressed=False)
            case.wait(lambda: case.ipc('diagnostics')['input_state']['pressed_keys'] == 0 and case.ipc('diagnostics')['input_state']['suppressed_keys'] == 0)
            require(not seen('keyboard.key', since=chord, key=30), 'configured shortcut press/release leaked to client')
            plain = len(events())
            inject('key', key='a', pressed=True); inject('key', key='a', pressed=False)
            case.wait(lambda: seen('keyboard.key', since=plain, key=30, state='pressed') and seen('keyboard.key', since=plain, key=30, state='released'))
            require(case.ipc('workspaces.list')['workspaces'][0]['mode'] == 'canvas', 'plain A unexpectedly changed workspace')
            case.result['shortcut_snapshot'] = case.snapshot('configured-shortcut')
            case.result['chord_event_index'] = chord
            case.result['plain_event_index'] = plain
            case.wait(lambda: client.poll() is not None, 9)
            require((case.directory / 'fixture.stdout.log').stat().st_size <= 1024 * 1024, 'fixture report exceeds bound')
            report = json.loads((case.directory / 'fixture.stdout.log').read_text())
            require(client.returncode == 0 and report.get('ok') is True, 'input fixture failed')
            journal = events()
            counts = {str(key): {state: sum(event.get('type') == 'keyboard.key' and event.get('key') == key and event.get('state') == state for event in journal)
                                 for state in ['pressed', 'released']} for key in [30, 56]}
            require(counts == {'30': {'pressed': 1, 'released': 1}, '56': {'pressed': 2, 'released': 2}}, 'complete fixture key ownership differs')
            case.result['complete_key_counts'] = counts
            case.result['fixture_report'] = report
        case.result['ok'] = True
    except Exception as error:
        case.result['error'] = str(error)
    finally:
        if case.process is not None and case.process.poll() is None:
            for key in ['a', 'Alt_L']:
                try:
                    session.test(case.process.pid, {'kind': 'key', 'key': key, 'pressed': False, 'activate': False})
                except Exception:
                    pass
        if client is not None and client.poll() is None:
            client.terminate()
            try:
                client.wait(timeout=2)
            except subprocess.TimeoutExpired:
                client.kill(); client.wait(timeout=2)
        for stream in fixture_streams:
            stream.close()
        case.close()
    return case.result


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--fixture', type=Path, help='defaults to fluxwm-fixture beside selected binary')
    parser.add_argument('--case', choices=['all', 'baseline', 'input-publication', 'path-retention'], default='all')
    parser.add_argument('--artifacts', type=Path)
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-config' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    parent = dict(os.environ)
    binary = args.binary.resolve()
    helper = binary.with_name('fluxwm-config')
    fixture = (args.fixture or binary.with_name('fluxwm-fixture')).resolve()
    result = {'ok': False, 'scope': 'private nested configuration publication/retention, held Alt deferral and configured key ownership; no host-session injection, native or sustained frame-progress qualification',
              'harness_sha256': sha256(Path(__file__)), 'cases': []}
    session = runtime_owner = None
    try:
        tested_paths = [binary, helper] + ([fixture] if args.case in ('all', 'input-publication') else [])
        result['executables'] = {str(path): sha256(path) for path in tested_paths}
        xvfb, result['package'] = prepare_xvfb(args.bundle, directory)
        session = Session(xvfb, directory, screen='3000x2000x24')
        until = time.monotonic() + 8
        while not session.ready():
            require(session.server.poll() is None and time.monotonic() < until, 'private display readiness failed')
            time.sleep(.01)
        require(Path('/tmp/.X' + str(session.number) + '-lock').read_text().strip() == str(session.server.pid), 'private display PID mismatch')
        require(os.getpgid(session.server.pid) == session.server.pid, 'private display session ownership mismatch')
        runtime_owner = short_runtime()
        environment = dict(session.environment, XDG_RUNTIME_DIR=runtime_owner.name)
        result['private_display'] = session.display
        if args.case in ('all', 'baseline'):
            result['cases'].extend(exercise(binary, directory, environment, name)
                                   for name in ['disabled', 'invalid-startup', 'reload-retention'])
        for name in ['input-publication', 'path-retention']:
            if args.case in ('all', name):
                result['cases'].append(extra_case(binary, fixture, directory, environment, session, name))
        result['ok'] = all(case['ok'] for case in result['cases'])
        result['executables_unchanged'] = all(sha256(Path(path)) == digest for path, digest in result['executables'].items())
        result['ok'] &= result['executables_unchanged']
    except Exception as error:
        result['error'] = str(error)
    finally:
        if session is not None:
            session.close()
            result['authority_removed'] = not session.auth.exists()
            result['server_reaped'] = session.server.poll() is not None
            result['x_socket_removed'] = not Path('/tmp/.X11-unix/X' + str(session.number)).exists()
        if runtime_owner is not None:
            result['runtime_cleanup'] = close_runtime(runtime_owner)
            result['ok'] &= all(result['runtime_cleanup'][key] for key in ['endpoints_removed_before_cleanup', 'directory_removed'])
        result['parent_environment_unchanged'] = dict(os.environ) == parent
        write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory), 'error': result.get('error')}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
