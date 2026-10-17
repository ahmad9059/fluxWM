"""One-off author driver: private startup/reload of the runnable nested example."""
import argparse
import json
import os
from pathlib import Path
import sys
import time

parser = argparse.ArgumentParser()
parser.add_argument('--repo', type=Path, default=Path.cwd())
parser.add_argument('--artifacts', type=Path, required=True)
args = parser.parse_args()
root = args.repo.resolve()
sys.path.insert(0, str(root / 'tools'))
from smoke_config import Case
from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import require, write_json, sha256

out = args.artifacts.resolve()
out.mkdir()
parent = dict(os.environ)
session = owner = case = None
result = {'ok': False, 'scope': 'Private startup/manual reload of the runnable nested example; no terminal argv execution or native qualification', 'driver_sha256': sha256(Path(__file__))}
try:
    binary = root / 'target/debug/fluxwm'
    helper = binary.with_name('fluxwm-config')
    example = root / 'docs/examples/nested-config.lua'
    result['executables'] = {str(p): sha256(p) for p in [binary, helper]}
    result['example_sha256'] = sha256(example)
    xvfb, result['package'] = prepare_xvfb(root / 'target/configure-verified-xvfb', out)
    session = Session(xvfb, out, screen='3000x2000x24')
    deadline = time.monotonic() + 8
    while not session.ready():
        require(session.server.poll() is None and time.monotonic() < deadline, 'private display startup failed')
        time.sleep(.01)
    require(Path('/tmp/.X' + str(session.number) + '-lock').read_text().strip() == str(session.server.pid), 'private server identity differs')
    owner = short_runtime()
    environment = dict(session.environment, XDG_RUNTIME_DIR=owner.name)
    case = Case(binary, out / 'example', environment, example.read_text())
    case.start()
    initial = case.await_status('applied')['configuration']['applied_generation']
    before = case.snapshot('startup')
    require(before['effective']['effective']['layout'] == {'gaps': 12.0, 'mode': 'strip'}, 'example layout not applied')
    require(before['diagnostics']['keyboard_bindings']['entries'] == 16 and before['diagnostics']['keyboard_bindings']['source'] == 'user-config', 'example table not installed')
    require(before['diagnostics']['owned_applications'] == 0 and before['diagnostics']['pending_launches'] == 0, 'evaluation spawned applications')
    latest = case.reload()
    case.await_status('applied', latest)
    after = case.snapshot('reload')
    require(after['effective']['effective'] == before['effective']['effective'], 'example reload changed settings')
    case.result['ok'] = True
    result['generations'] = [initial, latest]
    result['ok'] = True
except Exception as error:
    result['error'] = str(error)
finally:
    if case:
        case.close()
        result['case'] = case.result
        result['ok'] &= case.result['ok']
    if session:
        session.close()
        result.update(authority_removed=not session.auth.exists(), server_reaped=session.server.poll() is not None, x_socket_removed=not Path('/tmp/.X11-unix/X' + str(session.number)).exists())
    if owner:
        result['runtime_cleanup'] = close_runtime(owner)
        result['ok'] &= all(result['runtime_cleanup'][key] for key in ['directory_removed', 'endpoints_removed_before_cleanup'])
    result['parent_environment_unchanged'] = dict(os.environ) == parent
    result['executables_unchanged'] = all(sha256(Path(p)) == h for p, h in result.get('executables', {}).items())
    result['ok'] &= all(result.get(key) is True for key in ['authority_removed', 'server_reaped', 'x_socket_removed', 'parent_environment_unchanged', 'executables_unchanged'])
    write_json(out / 'summary.json', result)
print(json.dumps({'ok': result['ok'], 'error': result.get('error'), 'generations': result.get('generations')}))
raise SystemExit(0 if result['ok'] else 1)
