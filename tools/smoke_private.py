#!/usr/bin/env python3
"""Run owned nested regression suites on an authenticated isolated Xvfb."""
import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import uuid

from smoke_input import Session, prepare_xvfb, short_runtime, close_runtime
from smoke_nested import require, sha256, write_json


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--bundle', type=Path)
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--fixture', type=Path, default=root / 'target/debug/fluxwm-fixture')
    parser.add_argument('--suites', nargs='+', choices=['scene', 'nested', 'shutdown', 'ipc', 'health', 'configure', 'minimize', 'maximize'], default=['scene', 'nested', 'shutdown', 'ipc', 'health', 'configure', 'minimize', 'maximize'])
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-private' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    parent = dict(os.environ)
    result = {'ok': False, 'scope': 'owned authenticated X11 software-rendered nested suites; no host input injection', 'harness_sha256': sha256(Path(__file__)), 'suites': []}
    session = process = runtime_owner = None
    try:
        executable, verification = prepare_xvfb(args.bundle, directory)
        result['package'] = verification
        # Default Xvfb cursor starts at screen center. Keep it outside the default
        # 1280x800 test window without injecting input; the scene fixtures observe
        # the compositor's unchanged initial pointer at (0,0).
        session = Session(executable, directory, screen='3000x2000x24')
        deadline = time.monotonic() + 8
        while not session.ready():
            require(session.server.poll() is None and time.monotonic() < deadline, 'private server readiness failed')
            time.sleep(.01)
        require(Path('/tmp/.X' + str(session.number) + '-lock').read_text().strip() == str(session.server.pid), 'private display lock PID mismatch')
        require(os.getpgid(session.server.pid) == session.server.pid, 'private server session ownership mismatch')
        runtime_owner = short_runtime(); runtime = Path(runtime_owner.name)
        environment = dict(session.environment, XDG_RUNTIME_DIR=str(runtime))
        result['private_server_pid'] = session.server.pid
        result['private_display'] = session.display
        for suite in args.suites:
            script = {'scene': 'smoke_scene.py', 'nested': 'smoke_nested.py', 'shutdown': 'smoke_shutdown.py', 'ipc': 'smoke_ipc_runtime.py', 'health': 'smoke_health.py', 'configure': 'smoke_configure.py', 'minimize': 'smoke_minimize.py', 'maximize': 'smoke_maximize.py'}[suite]
            command = [sys.executable, str(root / 'tools' / script), '--binary', str(args.binary.resolve()), '--artifacts', str(directory / suite)]
            if suite in ('scene', 'nested', 'health', 'configure', 'minimize', 'maximize'): command += ['--fixture', str(args.fixture.resolve())]
            stdout, stderr = directory / (suite + '.stdout'), directory / (suite + '.stderr')
            start = time.monotonic()
            with stdout.open('wb') as out, stderr.open('wb') as err:
                process = subprocess.Popen(command, stdout=out, stderr=err, env=environment, start_new_session=True)
                while process.poll() is None:
                    require(time.monotonic() - start < 65, 'suite exceeded 65-second deadline')
                    require(all(p.stat().st_size < 4 * 1024 * 1024 for p in (stdout, stderr)), 'suite output budget exceeded')
                    time.sleep(.02)
            result['suites'].append({'suite': suite, 'command': command, 'exit_code': process.returncode, 'elapsed_seconds': round(time.monotonic() - start, 3)})
            require(process.returncode == 0, suite + ' failed; inspect retained suite artifacts')
            process = None
        result['ok'] = True
    except Exception as error:
        result['error'] = str(error)
    finally:
        if process is not None and process.poll() is None:
            process.terminate()
            try: process.wait(timeout=3)
            except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=2)
        if session is not None:
            session.close()
            result['authority_removed'] = not session.auth.exists()
            result['server_reaped'] = session.server.poll() is not None
            result['x_socket_removed'] = not Path('/tmp/.X11-unix/X' + str(session.number)).exists()
        if runtime_owner is not None:
            result['runtime_cleanup'] = close_runtime(runtime_owner)
            result['ok'] &= all(result['runtime_cleanup'][k] for k in ['endpoints_removed_before_cleanup', 'directory_removed'])
        result['parent_environment_unchanged'] = parent == dict(os.environ)
        write_json(directory / 'result.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory), 'error': result.get('error')}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
