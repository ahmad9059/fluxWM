#!/usr/bin/env python3
"""Real autostart lifecycle on an authenticated, owned private Xvfb."""
import argparse
import json
import os
from pathlib import Path
import time

from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import sha256
from smoke_config import Case
from smoke_nested import require, write_json

FIXTURE = '''import json,os,signal,sys,time
folder,tag,policy=sys.argv[1:]
def event(kind):
    path=os.path.join(folder,str(os.getpid())+'-'+kind+'.json')
    tmp=path+'.next'
    with open(tmp,'w') as f:
        json.dump(dict(pid=os.getpid(),tag=tag,kind=kind,wayland=os.getenv('WAYLAND_DISPLAY'),ipc=os.getenv('FLUXWM_SOCKET'),display=os.getenv('DISPLAY'),socket=os.getenv('WAYLAND_SOCKET'),blocked=sorted(int(s) for s in signal.pthread_sigmask(signal.SIG_BLOCK,[]))),f)
    os.replace(tmp,path)
def terminate(*_):
    event('term')
    if policy != 'ignore': sys.exit(0)
signal.signal(signal.SIGTERM,terminate)
event('ready')
if policy != 'fast': time.sleep(10)
'''


def source(entries, invalid=False):
    commands = []
    for ident, argv, restart in entries:
        commands.append('{id=%s,argv={%s},restart_on_change=%s}' % (
            json.dumps(ident), ','.join(json.dumps(a) for a in argv), str(restart).lower()))
    chord = 'Super+FluxwmDefinitelyUnknownKeysym' if invalid else 'Super+r'
    return 'return {schema_version=1,bindings={{key={sym=%s},action={name="reload-config"}}},autostart={%s}}\n' % (json.dumps(chord), ','.join(commands))


def identity(pid):
    path = Path('/proc') / str(pid) / 'stat'
    try:
        return path.read_text().rsplit(')', 1)[1].split()[19]
    except FileNotFoundError:
        return None


def run(binary, directory, environment, shutdown):
    directory.mkdir(mode=0o700)
    folder = directory / 'events'; folder.mkdir(mode=0o700)
    script = directory / 'companion.py'; script.write_text(FIXTURE)
    def entry(ident, tag, restart=False, policy='long'):
        return (ident, ['/usr/bin/python3', str(script), str(folder), tag, policy], restart)
    a, b, fast = entry('a', 'a'), entry('b', 'b', True, 'ignore'), entry('fast', 'fast', True, 'fast')
    missing = ('missing', ['/missing/SECRET_AUTOSTART_ARGV'], True)
    case = Case(binary, directory / 'lifecycle', environment, source([a, b, fast, missing]))
    owned = {}
    stages = []
    shutdown_started = None
    def settle(g):
        def ready():
            v = case.ipc('diagnostics')['autostart']
            return v if v['settled'] and v['observed_generation'] == g else None
        v = case.wait(ready)
        require(v['requested_generation'] == g and v['active_generation'] == g and v['error'] is None, 'autostart generation/error differs')
        rows = {e['id']: e for e in v['entries']}
        for row in rows.values():
            if row['pid'] is not None:
                owned.setdefault(row['pid'], identity(row['pid']))
        stages.append(v)
        return rows
    def reload(entries, invalid=False):
        case.replace(source(entries, invalid)); g = case.reload()
        case.await_status('failed' if invalid else 'applied', g)
        return g
    def events():
        result = [json.loads(p.read_text()) for p in folder.glob('*-ready.json')]
        require(len(result) <= 32, 'duplicate launch budget exceeded')
        return result
    def gone(pid):
        return identity(pid) != owned[pid]
    try:
        case.start()
        g = case.await_status('applied')['configuration']['applied_generation']
        first = settle(g)
        case.wait(lambda: len(events()) == 3)
        original = {key: first[key]['pid'] for key in ('a', 'b')}
        require(all(original.values()), 'initial companions missing')
        require(first['missing']['state'] == 'failed' and first['missing']['errno'] == 2 and 'SECRET' not in json.dumps(first), 'failed exec diagnostic leaked source or lost errno')
        def fast_exited():
            row = {e['id']: e for e in case.ipc('diagnostics')['autostart']['entries']}['fast']
            return row if row['state'] == 'exited' and row['exit_code'] == 0 else None
        case.wait(fast_exited)
        same = settle(reload([b, fast, a, missing]))
        require(all(same[k]['pid'] == original[k] for k in original), 'named reorder replaced live companion')
        require(all(e['attempts'] == '1' for e in same.values()) and len(events()) == 3, 'unchanged reload retried exited/failed child')
        changed = entry('a', 'changed', False)
        deferred = settle(reload([changed, b, fast, missing]))
        require(deferred['a']['pid'] == original['a'] and deferred['a']['change_deferred'], 'restart false did not retain original')
        changed = entry('a', 'changed', True)
        restarted = settle(reload([changed, b, fast, missing]))
        replacement = restarted['a']['pid']
        require(replacement and replacement != original['a'] and restarted['a']['attempts'] == '2' and gone(original['a']), 'changed argv did not retire and replace child')
        case.wait(lambda: (folder / (str(replacement) + '-ready.json')).exists())
        before = case.ipc('config.effective')
        reload([('forbidden', ['/usr/bin/touch', str(case.marker)], True)], invalid=True)
        require(case.ipc('config.effective') == before and not case.marker.exists(), 'rejected candidate changed logical state or executed')
        retained = case.ipc('diagnostics')['autostart']
        require(retained['active_generation'] == stages[-1]['active_generation'], 'rejected generation reached autostart')
        removed = settle(reload([b, fast, missing]))
        require(removed['a']['state'] == 'removed' and gone(replacement), 'removal retained live child')
        suppressed = settle(reload([a, b, fast, missing]))
        require(suppressed['a']['pid'] is None and suppressed['a']['attempts'] == '2', 'once-only reintroduction retried')
        a = entry('a', 'returned', True)
        recovered = settle(reload([a, b, fast, ('missing', ['/bin/true'], True)]))
        require(recovered['a']['pid'] and recovered['a']['attempts'] == '3' and recovered['missing']['attempts'] == '2', 'explicit restart/recovery failed')
        case.wait(lambda: len(events()) == 5)
        for e in events():
            require(e['wayland'] == case.instance and e['ipc'] == str(case.ipc_path), 'child display/socket identity differs')
            require(e['display'] is None and e['socket'] is None and not set(e['blocked']) & {2, 15, 17}, 'stale environment or blocked termination signal inherited')
        case.result.update(ok=True, shutdown_method=shutdown, stages=stages, events=events(), owned_processes=owned)
        if shutdown == 'ipc-quit':
            shutdown_started = time.monotonic()
            accepted = case.ipc('action', {'name': 'quit'})
            require(accepted == {'accepted': True}, 'quit reply not acknowledged')
            case.result['quit_reply'] = accepted
            until = time.monotonic() + 2
            while case.process.poll() is None:
                require(time.monotonic() < until, 'quit failed to finish owned teardown')
                time.sleep(.01)
    except Exception as error:
        case.result['error'] = str(error)
    finally:
        started = shutdown_started or time.monotonic(); case.close()
        case.result['shutdown_seconds'] = time.monotonic() - started
        case.result['all_observed_processes_gone'] = all(gone(pid) for pid in owned)
        if case.stats.exists():
            final = json.loads(case.stats.read_text())['autostart']
            case.result['final_autostart'] = final
            rows = {e['id']: e for e in final['entries']}
            case.result['term_ignoring_child_killed'] = rows.get('b', {}).get('exit_signal') == 9 and (folder / (str(original['b']) + '-term.json')).exists()
            case.result['ok'] &= case.result['term_ignoring_child_killed']
            case.result['ok'] &= final['closed'] and final['settled'] and all(e['pid'] is None for e in final['entries'])
        else:
            case.result['ok'] = False
        case.result['ok'] &= case.result['all_observed_processes_gone'] and case.result['shutdown_seconds'] < 2
        write_json(case.directory / 'result.json', case.result)
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
    result = {'ok': False, 'scope': 'Named companion lifecycle, once-only reload, restart, removal, exec failure and owned shutdown; no native/session/descendant/soak qualification'}
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
        result['cases'] = [run(binaries[0], directory / mode, environment, mode) for mode in ['sigterm', 'ipc-quit']]
        result['ok'] = all(case['ok'] for case in result['cases'])
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
    print(json.dumps({'ok': result['ok'], 'error': result.get('error') or next((case.get('error') for case in result.get('cases', []) if not case['ok']), None), 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1

if __name__ == '__main__':
    raise SystemExit(main())
