#!/usr/bin/env python3
"""Configured future-column widths on an authenticated owned private display."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import time

from smoke_config import Case
from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import require, sha256, write_json


def source(default, first_preset=400, invalid=False):
    return '''return {schema_version=1,layout={gaps=%s,mode="%s",default_width=%s,
      width_presets={{logical_px=%s},{fraction=0.25}}},
      bindings={{key={sym="Super+r"},action={name="reload-config"}},
      {key={sym="Super+q"},action={name="close-window"}}}}\n''' % (21 if invalid else 12, 'canvas' if invalid else 'strip', default, first_preset)


def run(binary, fixture, directory, environment):
    case = Case(binary, directory / 'widths', environment, source('{preset=1}'))
    client = None
    logs = []
    widths = []
    identities = {}
    try:
        case.start()
        initial = case.await_status('applied')['configuration']['applied_generation']
        viewport = case.ipc('outputs.list')['outputs'][0]['geometry']['width']
        widths = [400, 760, viewport * .25, 600, 600, 480]
        child_environment = dict(environment, WAYLAND_DISPLAY=case.instance, FLUXWM_SOCKET=str(case.ipc_path))
        logs = [(case.directory / n).open('wb') for n in ['fixture.stdout.log', 'fixture.stderr.log']]
        command = [str(fixture), 'config-width', '--timeout-ms', '10000']
        write_json(case.directory / 'fixture-command.json', {'command': command})
        client = subprocess.Popen(command, env=child_environment, stdout=logs[0], stderr=logs[1], start_new_session=True)

        def members(count):
            require(client.poll() is None, 'width fixture exited before observation')
            windows = case.ipc('windows.list')['windows']
            named = {w['title']: w for w in windows if w['mapped']}
            if len(named) != count or any('width-%s-ready' % i not in named for i in range(count)):
                return None
            for i in range(count):
                item = named['width-%s-ready' % i]
                require(abs(item['geometry']['width'] - widths[i]) < .0001, 'wrong configured width for member%s' % i)
                if i in identities:
                    require(item['id'] == identities[i], 'existing member identity changed')
                identities[i] = item['id']
            return windows

        case.result['initial_windows'] = case.wait(lambda: members(1))
        for count, default, preset in [(2, '{logical_px=760}', 400), (3, '{fraction=0.25}', 400), (4, '{preset=1}', 600)]:
            case.replace(source(default, preset))
            accepted = case.reload()
            case.await_status('applied', accepted)
            snapshot = case.snapshot('applied-%s' % count)
            expected = {'logical_px': widths[count-1]} if count != 3 else {'fraction': .25}
            require(snapshot['effective']['effective']['layout']['default_width'] == expected, 'resolved effective default differs')
            require(snapshot['effective']['desired']['layout']['default_width'] == ({'logical_px': 760} if count == 2 else {'fraction': .25} if count == 3 else {'preset': 1}), 'desired width reference lost')
            case.result['windows-%s' % count] = case.wait(lambda: members(count))
        applied = case.configuration()['configuration']['applied_generation']
        old = case.ipc('config.effective')
        case.replace(source('{preset=99}', invalid=True))
        rejected = case.reload()
        failed = case.await_status('failed', rejected)
        require(failed['configuration']['applied_generation'] == applied and failed['configuration']['last_error'], 'invalid preset replaced applied generation')
        require(case.ipc('config.effective') == old, 'invalid preset changed desired/effective state')
        case.result['after-failed-reload'] = case.wait(lambda: members(5))
        case.replace(source('{logical_px=480}'))
        recovered = case.reload()
        case.await_status('applied', recovered)
        case.result['recovered_windows'] = case.wait(lambda: members(6))
        case.wait(lambda: client.poll() is not None, 4)
        require((case.directory / 'fixture.stdout.log').stat().st_size < 1024 * 1024, 'fixture report too large')
        report = json.loads((case.directory / 'fixture.stdout.log').read_text())
        require(client.returncode == 0 and report['ok'], 'fixture protocol failure')
        require(len(report['windows']) == 6 and all(w['frames'] >= 1 for w in report['windows']), 'initial frames missing')
        for i, window in enumerate(report['windows']):
            require(window['configure_sizes'][-1][0] == round(widths[i]), 'wire configure width differs for member%s' % i)
        case.result.update(ok=True, fixture_report=report, expected_widths=widths, generations=[initial, applied, rejected, recovered])
    except Exception as error:
        case.result['error'] = str(error)
    finally:
        if client and client.poll() is None:
            client.terminate()
            try:
                client.wait(timeout=2)
            except subprocess.TimeoutExpired:
                client.kill(); client.wait(timeout=2)
        for log in logs:
            log.close()
        case.close()
    return case.result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/debug/fluxwm'))
    parser.add_argument('--fixture', type=Path, default=Path('target/debug/fluxwm-fixture'))
    parser.add_argument('--bundle', type=Path, required=True)
    parser.add_argument('--artifacts', type=Path, required=True)
    args = parser.parse_args()
    directory = args.artifacts.resolve(); directory.mkdir()
    parent = dict(os.environ)
    result = {'ok': False, 'scope': 'Configured future-column widths, retained existing columns, invalid-preset retention and real configure/frame events; no native/presentation/soak qualification'}
    session = owner = None
    try:
        binaries = [args.binary.resolve(), args.fixture.resolve(), args.binary.resolve().with_name('fluxwm-config')]
        result['executables'] = {str(p): sha256(p) for p in binaries}
        result['harness_sha256'] = sha256(Path(__file__))
        xvfb, result['package'] = prepare_xvfb(args.bundle, directory)
        session = Session(xvfb, directory, screen='3000x2000x24')
        deadline = time.monotonic() + 8
        while not session.ready():
            require(session.server.poll() is None and time.monotonic() < deadline, 'private display readiness failed')
            time.sleep(.01)
        require(Path('/tmp/.X' + str(session.number) + '-lock').read_text().strip() == str(session.server.pid), 'private server identity differs')
        owner = short_runtime()
        environment = dict(session.environment, XDG_RUNTIME_DIR=owner.name)
        result['case'] = run(binaries[0], binaries[1], directory, environment)
        result['ok'] = result['case']['ok']
        result['executables_unchanged'] = all(sha256(Path(p)) == h for p,h in result['executables'].items())
    except Exception as error:
        result['error'] = str(error)
    finally:
        if session:
            session.close()
            result.update(authority_removed=not session.auth.exists(), server_reaped=session.server.poll() is not None, x_socket_removed=not Path('/tmp/.X11-unix/X' + str(session.number)).exists())
        if owner:
            result['runtime_cleanup'] = close_runtime(owner)
            result['ok'] &= all(result['runtime_cleanup'][key] for key in ['directory_removed','endpoints_removed_before_cleanup'])
        result['parent_environment_unchanged'] = dict(os.environ) == parent
        result['ok'] &= all(result.get(key) is True for key in ['authority_removed','server_reaped','x_socket_removed','parent_environment_unchanged','executables_unchanged'])
        write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'error': result.get('error'), 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
