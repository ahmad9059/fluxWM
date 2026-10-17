#!/usr/bin/env python3
"""Real host-focus cancellation tests on an authenticated owned Xvfb only."""
import argparse
import json
import os
from pathlib import Path
import shutil
import socket
import struct
import subprocess
import sys
import time
import uuid

from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import require, sha256, write_json


def run_case(executable, binary, fixture, directory, case):
    directory.mkdir(mode=0o700)
    result = {'ok': False, 'case': case, 'errors': [], 'binary_sha256': sha256(binary), 'fixture_sha256': sha256(fixture)}
    session = compositor = client = runtime_owner = None
    logs = []
    deadline = time.monotonic() + 20
    responses = []
    event_path = directory / 'input.events.jsonl'
    def check():
        require(time.monotonic() < deadline, 'case deadline expired')
        require(session.server.poll() is None, 'private X server exited')
        require(compositor is None or compositor.poll() is None, 'compositor exited early')
        for p in directory.glob('*.log'): require(p.stat().st_size < 4 * 1024 * 1024, 'log budget exceeded')
    def events():
        if not event_path.exists(): return []
        require(event_path.stat().st_size <= 1024 * 1024, 'event file budget exceeded')
        raw = event_path.read_text(); lines = raw.splitlines()
        if raw and not raw.endswith('\n'): lines = lines[:-1]
        require(len(lines) <= 1024, 'event count budget exceeded')
        return [json.loads(line)['event'] for line in lines]
    def seen(kind, since=0, **values):
        return any(e.get('type') == kind and all(e.get(k) == v for k, v in values.items()) for e in events()[since:])
    def wait(predicate, timeout=3):
        stop = min(deadline, time.monotonic() + timeout)
        while not predicate():
            check(); require(time.monotonic() < stop, 'event observation expired'); time.sleep(.01)
    def ipc(method, params=None):
        check(); ident = uuid.uuid4().hex
        payload = json.dumps({'version': 1, 'id': ident, 'method': method, 'params': params or {}}).encode()
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(min(2, max(.001, deadline - time.monotonic())))
            connection.connect(str(ipc_path)); connection.sendall(struct.pack('>I', len(payload)) + payload)
            def exact(count):
                raw = b''
                while len(raw) < count:
                    check(); chunk = connection.recv(count - len(raw)); require(chunk, 'IPC EOF'); raw += chunk
                return raw
            size, = struct.unpack('>I', exact(4)); require(0 < size <= 1024 * 1024, 'IPC reply bound')
            response = json.loads(exact(size)); require(response['id'] == ident and 'result' in response, 'IPC operation rejected')
        responses.append(response); return response['result']
    def inject(kind, **values):
        check(); return session.test(compositor.pid, {'kind': kind, **values}, timeout=min(2, max(.001, deadline - time.monotonic())))
    def expect(condition, message):
        if not condition: result['errors'].append(message)
    def observe(predicate, timeout=1):
        stop = min(deadline, time.monotonic() + timeout)
        while not predicate() and time.monotonic() < stop: check(); time.sleep(.01)
        return predicate()
    try:
        session = Session(executable, directory)
        wait(session.ready)
        runtime_owner = short_runtime(); runtime = Path(runtime_owner.name)
        environment = dict(session.environment, XDG_RUNTIME_DIR=str(runtime), XKB_DEFAULT_LAYOUT='us', XKB_DEFAULT_VARIANT='', XKB_DEFAULT_OPTIONS='')
        name = 'fluxfocus-' + uuid.uuid4().hex[:16]
        ready, stats = directory / 'ready.json', directory / 'stats.json'
        for filename in ['compositor.stdout.log', 'compositor.stderr.log', 'fixture.stdout.log', 'fixture.stderr.log']: logs.append((directory / filename).open('wb'))
        command = [str(binary), '--no-config', '--socket', name, '--ready-file', str(ready), '--stats-file', str(stats), '--exit-after-ms', '18000']
        write_json(directory / 'command.json', {'compositor': command, 'fixture': [str(fixture), 'input-probe', '--timeout-ms', '10000']})
        compositor = subprocess.Popen(command, env=environment, stdout=logs[0], stderr=logs[1], start_new_session=True)
        wait(ready.exists, 8)
        publication = json.loads(ready.read_text()); require(publication['pid'] == compositor.pid and publication['wayland_display'] == name, 'readiness identity mismatch')
        ipc_path = Path(publication['ipc_socket']); require(ipc_path == runtime / 'fluxwm' / (name + '.sock'), 'IPC path mismatch')
        result['target'] = session.test(compositor.pid)
        inject('focus', focused=True)
        child_environment = dict(environment, WAYLAND_DISPLAY=name, FLUXWM_SOCKET=str(ipc_path), FLUXWM_FIXTURE_EVENT_FILE=str(event_path))
        child_environment.pop('FLUXWM_FIXTURE_GRAB_POPUP', None)
        if case == 'popup': child_environment['FLUXWM_FIXTURE_GRAB_POPUP'] = '1'
        client = subprocess.Popen([str(fixture), 'input-probe', '--timeout-ms', '10000'], env=child_environment, stdout=logs[2], stderr=logs[3], start_new_session=True)
        wait(lambda: seen('ready'), 8)
        owners = {w['title']: w for w in ipc('windows.list')['windows']}
        require(set(owners) == {'input-A', 'input-B'}, 'fixture inventory differs')
        if not owners['input-A']['focused']: ipc('action', {'name': 'focus', 'direction': 'left'})
        wait(lambda: seen('keyboard.enter', root=0))
        inject('motion', x=64, y=64)
        wait(lambda: seen('pointer.enter', root=0))
        if case == 'keyboard':
            inject('key', key='Alt_L', pressed=True)
            wait(lambda: seen('keyboard.key', key=56, state='pressed'))
            inject('key', key='c', pressed=True)
            wait(lambda: ipc('workspaces.list')['workspaces'][0]['mode'] == 'canvas')
            loss = len(events())
            inject('focus', focused=False)
            cancelled = observe(lambda: seen('keyboard.key', since=loss, key=56, state='released'))
            result['focus_loss_event_index'] = loss
            result['focus_cancel_end_index'] = len(events())
            expect(cancelled, 'focus_loss_missing_Alt_release')
            modifiers = [e for e in events()[loss:] if e.get('type') == 'keyboard.modifiers']
            expect(bool(modifiers) and modifiers[-1].get('depressed') == 0, 'depressed_modifiers_survived_focus_loss')
            expect(not any(e.get('type') == 'keyboard.key' and e.get('key') == 46 for e in events()), 'consumed_C_leaked_to_client')
            result['after_loss'] = ipc('diagnostics')
            # Release physically on the private root, outside the FluxWM window.
            inject('key', key='c', pressed=False, activate=False)
            inject('key', key='Alt_L', pressed=False, activate=False)
            inject('focus', focused=True)
            plain = len(events())
            inject('key', key='s', pressed=True); inject('key', key='s', pressed=False)
            delivered = observe(lambda: seen('keyboard.key', since=plain, key=31, state='pressed') and seen('keyboard.key', since=plain, key=31, state='released'))
            expect(delivered, 'plain_S_missing_after_focus_return')
            result['plain_S_workspace'] = ipc('workspaces.list')
            expect(result['plain_S_workspace']['workspaces'][0]['mode'] == 'canvas', 'stale_Alt_triggered_workspace_shortcut')
            # Fresh physical chord must work again after the outside releases.
            inject('key', key='Alt_L', pressed=True); inject('key', key='s', pressed=True)
            wait(lambda: ipc('workspaces.list')['workspaces'][0]['mode'] == 'strip')
            inject('key', key='s', pressed=False); inject('key', key='Alt_L', pressed=False)
            # A forwarded key held through re-entry must get exactly one cancel
            # release, and its eventual real release must not be forwarded twice.
            held = len(events()); inject('key', key='a', pressed=True)
            wait(lambda: seen('keyboard.key', since=held, key=30, state='pressed'))
            inject('focus', focused=False)
            expect(observe(lambda: seen('keyboard.key', since=held, key=30, state='released')), 'held_A_not_cancelled')
            inject('focus', focused=True)
            late = len(events()); inject('key', key='a', pressed=False)
            time.sleep(.15); check()
            expect(not seen('keyboard.key', since=late, key=30, state='released'), 'late_A_release_forwarded_twice')
            fresh = len(events()); inject('key', key='a', pressed=True); inject('key', key='a', pressed=False)
            expect(observe(lambda: seen('keyboard.key', since=fresh, key=30, state='pressed') and seen('keyboard.key', since=fresh, key=30, state='released')), 'fresh_A_failed_after_reentry')
            result['consumed_reentry_tested'] = not result['errors']
            if result['consumed_reentry_tested']:
                inject('key', key='Alt_L', pressed=True); inject('key', key='c', pressed=True)
                wait(lambda: ipc('workspaces.list')['workspaces'][0]['mode'] == 'canvas')
                inject('focus', focused=False)
                inject('key', key='Alt_L', pressed=False, activate=False)
                inject('focus', focused=True)
                inject('key', key='c', pressed=False)
        elif case == 'popup':
            inject('button', button=1, pressed=True)
            wait(lambda: seen('popup.frame', popup=0) and seen('keyboard.enter', popup=0))
            inject('motion', x=65, y=64)
            wait(lambda: seen('pointer.enter', popup=0))
            inject('key', key='a', pressed=True)
            wait(lambda: seen('keyboard.key', key=30, popup=0, state='pressed'))
            time.sleep(.2); check()
            before = ipc('diagnostics'); result['before_popup_loss'] = before
            expect(before.get('composed_scene', {}).get('popup_surfaces') == 1, 'popup_not_in_published_scene')
            expect(before.get('input_state', {}).get('keyboard_grabbed') is True and before.get('input_state', {}).get('pointer_grabbed') is True, 'valid_popup_grabs_not_installed')
            loss = len(events()); result['focus_loss_event_index'] = loss
            inject('focus', focused=False)
            expect(observe(lambda: seen('popup.done', since=loss, popup=0)), 'popup_not_dismissed_on_focus_loss')
            expect(observe(lambda: seen('keyboard.key', since=loss, popup=0, key=30, state='released')), 'popup_held_key_not_released')
            expect(observe(lambda: seen('keyboard.enter', since=loss, root=0, keys=[])), 'root_keyboard_not_restored_empty')
            # The fixture retains the dismissed popup role and sends no further
            # Wayland request until S below, so client teardown cannot mask redraw.
            after = ipc('diagnostics')
            stop = time.monotonic() + .5
            while after['runtime']['compositions'] <= before['runtime']['compositions'] and time.monotonic() < stop:
                check(); time.sleep(.01); after = ipc('diagnostics')
            result['after_popup_loss'] = after; result['after_loss'] = after
            expect(after['runtime']['compositions'] > before['runtime']['compositions'] and after.get('composed_scene', {}).get('popup_surfaces') == 0, 'dismissed_popup_not_recomposed')
            expect(after['runtime']['wayland_requests'] == before['runtime']['wayland_requests'], 'client_request_masked_popup_recomposition')
            inject('focus', focused=True)
            inject('key', key='s', pressed=True); inject('key', key='s', pressed=False)
            expect(observe(lambda: seen('popup.stale_grab_requested', popup=1) and seen('popup.done', popup=1)), 'expired_popup_serial_not_rejected')
            expect(not seen('keyboard.enter', popup=1), 'denied_popup_stole_keyboard_focus')
            denied = ipc('diagnostics'); result['after_stale_grab'] = denied
            expect(denied['runtime'].get('popup_grabs_denied_missing_press') == after['runtime'].get('popup_grabs_denied_missing_press', 0) + 1, 'stale_grab_denied_for_wrong_reason')
            inject('key', key='a', pressed=False)
            inject('button', button=1, pressed=False)
            # Positive control: the same root must still authorize a fresh grab.
            inject('button', button=1, pressed=True)
            accepted = observe(lambda: seen('popup.frame', popup=2) and seen('keyboard.enter', popup=2))
            expect(accepted, 'fresh_popup_grab_after_return_failed')
            if accepted:
                inject('motion', x=66, y=64); wait(lambda: seen('pointer.enter', popup=2))
                fresh = ipc('diagnostics'); result['fresh_popup_grab'] = fresh
                expect(fresh['input_state']['keyboard_grabbed'] and fresh['input_state']['pointer_grabbed'], 'fresh_popup_wrappers_missing')
                inject('focus', focused=False)
                expect(observe(lambda: seen('popup.done', popup=2)), 'fresh_popup_not_cancelled')
                inject('button', button=1, pressed=False, activate=False)
        else:
            inject('button', button=1, pressed=True)
            wait(lambda: seen('pointer.button', root=0, button=272, state='pressed'))
            loss = len(events()); inject('focus', focused=False)
            cancelled = observe(lambda: seen('pointer.button', since=loss, root=0, button=272, state='released'))
            expect(cancelled, 'focus_loss_missing_button_release')
            result['focus_loss_event_index'] = loss
            result['focus_cancel_end_index'] = len(events())
            result['after_loss'] = ipc('diagnostics')
            inject('focus', focused=True)
            owners = {w['title']: w for w in ipc('windows.list')['windows']}
            delta = owners['input-B']['geometry']['x'] - owners['input-A']['geometry']['x']
            require(delta == int(delta) and 100 < delta < 1200, 'invalid controlled target displacement')
            inject('motion', x=int(delta) + 64, y=64)
            entered = observe(lambda: seen('pointer.enter', since=loss, root=1))
            expect(entered, 'old_click_grab_survived_focus_loss')
            release = len(events()); inject('button', button=1, pressed=False)
            time.sleep(.15); check()
            expect(not seen('pointer.button', since=release, state='released'), 'late_physical_release_became_duplicate_or_orphan')
            fresh = len(events()); inject('button', button=1, pressed=True); inject('button', button=1, pressed=False)
            expect(observe(lambda: seen('pointer.button', since=fresh, root=1, button=272, state='pressed') and seen('pointer.button', since=fresh, root=1, button=272, state='released')), 'fresh_B_click_failed')
        state = result['after_loss'].get('input_state', {})
        expect(state.get('host_focus') is False, 'host_focus_loss_not_observed')
        expect(state.get('pressed_keys') == 0 and state.get('suppressed_keys') == 0 and state.get('authorized_buttons') == 0 and state.get('pointer_grabbed') is False and state.get('keyboard_grabbed') is False, 'held_state_survived_focus_loss')
        result['final_input_state'] = ipc('diagnostics').get('input_state')
        wait(lambda: client.poll() is not None, 9)
        result['fixture_report'] = json.loads((directory / 'fixture.stdout.log').read_text())
        require(client.returncode == 0 and result['fixture_report']['ok'], 'fixture instrumentation failed')
        journal = events()
        if case == 'keyboard':
            counts = {str(key): {kind: sum(e.get('type') == 'keyboard.key' and e.get('key') == key and e.get('state') == kind for e in journal) for kind in ['pressed', 'released']} for key in [30, 31, 46, 56]}
            result['complete_key_counts'] = counts
            alt_count = 3 if result.get('consumed_reentry_tested') else 2
            expect(counts == {'30': {'pressed': 2, 'released': 2}, '31': {'pressed': 1, 'released': 1}, '46': {'pressed': 0, 'released': 0}, '56': {'pressed': alt_count, 'released': alt_count}}, 'complete_key_ownership_counts_differ')
        elif case == 'pointer':
            enter = next((i for i,e in enumerate(journal) if i >= result['focus_loss_event_index'] and e.get('type') == 'pointer.enter' and e.get('root') == 1), None)
            press = next((i for i,e in enumerate(journal) if e.get('type') == 'pointer.button' and e.get('root') == 1 and e.get('state') == 'pressed'), None)
            expect(enter is not None and press is not None and enter < press, 'B_press_preceded_pointer_enter')
            counts = {str(owner): {kind: sum(e.get('type') == 'pointer.button' and e.get('button') == 272 and e.get('root') == owner and e.get('state') == kind for e in journal) for kind in ['pressed', 'released']} for owner in [0, 1]}
            result['complete_button_counts'] = counts
            expect(counts == {'0': {'pressed': 1, 'released': 1}, '1': {'pressed': 1, 'released': 1}}, 'complete_button_ownership_counts_differ')
        else:
            def ownership(kind, code_name, code):
                found = {}
                for e in journal:
                    if e.get('type') != kind or e.get(code_name) != code: continue
                    owner = 'popup-' + str(e['popup']) if e.get('popup') is not None else 'root-' + str(e.get('root'))
                    key = owner + ':' + e.get('state', 'unknown'); found[key] = found.get(key, 0) + 1
                return found
            keys_a = ownership('keyboard.key', 'key', 30)
            keys_s = ownership('keyboard.key', 'key', 31)
            buttons = ownership('pointer.button', 'button', 272)
            result['popup_complete_ownership'] = {'A': keys_a, 'S': keys_s, 'left_button': buttons}
            expect(keys_a == {'popup-0:pressed': 1, 'popup-0:released': 1}, 'popup_A_ownership_count_differs')
            expect(keys_s == {'root-0:pressed': 1, 'root-0:released': 1}, 'popup_S_ownership_count_differs')
            expect(buttons == {'root-0:pressed': 2, 'popup-0:released': 1, 'popup-2:released': 1}, 'popup_button_ownership_count_differs')
        final_state = ipc('diagnostics').get('input_state', {})
        expect(final_state.get('pressed_keys') == 0 and final_state.get('authorized_buttons') == 0 and final_state.get('suppressed_keys') == 0, 'held_state_remained_after_fixture_completion')
        result['quit'] = ipc('action', {'name': 'quit'})
        compositor.wait(timeout=2); require(compositor.returncode == 0, 'compositor shutdown failed')
        result['stats'] = json.loads(stats.read_text())
        result['ok'] = not result['errors']
    except Exception as error:
        result['error'] = str(error)
    finally:
        if session is not None and compositor is not None and compositor.poll() is None:
            for key in ['a', 'c', 's', 'Alt_L', 'Shift_L']:
                try: session.test(compositor.pid, {'kind': 'key', 'key': key, 'pressed': False, 'activate': False})
                except Exception: pass
            try: session.test(compositor.pid, {'kind': 'button', 'button': 1, 'pressed': False, 'activate': False})
            except Exception: pass
        for child in [client, compositor]:
            if child is not None and child.poll() is None:
                child.terminate()
                try: child.wait(timeout=2)
                except subprocess.TimeoutExpired: child.kill(); child.wait(timeout=2)
        if session is not None:
            session.close()
            result['authority_removed'] = not session.auth.exists()
            result['private_server_reaped'] = session.server.poll() is not None
            result['private_x_socket_removed'] = not Path('/tmp/.X11-unix/X' + str(session.number)).exists()
            result['parent_environment_unchanged'] = session.parent == dict(os.environ)
        if runtime_owner is not None:
            result['runtime_cleanup'] = close_runtime(runtime_owner)
            result['ok'] &= all(result['runtime_cleanup'][k] for k in ['endpoints_removed_before_cleanup', 'directory_removed'])
        for log in logs: log.close()
        result['responses'] = responses
        result['compositor_exit'] = compositor.returncode if compositor else None
        result['fixture_exit'] = client.returncode if client else None
        write_json(directory / 'result.json', result)
    return result


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--fixture', type=Path, default=root / 'target/debug/fluxwm-fixture')
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--case', choices=['all', 'keyboard', 'pointer', 'popup'], default='all')
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-focus' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    source_hashes = {'harness': sha256(Path(__file__)), 'helper': sha256(root / 'tools/smoke_input.py')}
    executable, verification = prepare_xvfb(args.bundle, directory)
    identities = {}
    for source, name in [(args.binary.resolve(), 'fluxwm'), (args.fixture.resolve(), 'fixture')]:
        digest = sha256(source); shutil.copyfile(source, directory / name); (directory / name).chmod(0o700)
        require(digest == sha256(source) == sha256(directory / name), 'executable changed during snapshot')
        identities[name] = digest
    names = ['keyboard', 'pointer', 'popup'] if args.case == 'all' else [args.case]
    cases = [run_case(executable, directory / 'fluxwm', directory / 'fixture', directory / case, case) for case in names]
    result = {'ok': all(case['ok'] for case in cases), 'cases': cases, 'package': verification, 'executables': identities,
              'harness_sha256': source_hashes['harness'], 'private_helper_sha256': source_hashes['helper'],
              'test_sources_unchanged': source_hashes == {'harness': sha256(Path(__file__)), 'helper': sha256(root / 'tools/smoke_input.py')},
              'scope': 'authenticated owned X11 host focus and key/button cancellation; no host-session input, native devices or full grab matrix'}
    result['ok'] &= result['test_sources_unchanged']
    write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
