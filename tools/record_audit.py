#!/usr/bin/env python3
"""Record bounded local verification. Passing checks do not qualify a desktop."""
from __future__ import annotations
import argparse
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
COMMANDS = [
    ('toolchain', ['rustc', '-Vv']),
    ('native-metadata', ['pkg-config', '--modversion', 'wayland-server', 'xkbcommon', 'libudev', 'gbm', 'egl', 'glesv2']),
    ('check', ['cargo', 'check', '--workspace', '--locked']),
    ('tests', ['cargo', 'test', '--workspace', '--locked']),
    ('ipc-defect-reproduction', ['cargo', 'run', '--locked', '--manifest-path', 'docs/evidence/ipc-audit-probe/Cargo.toml', '--target-dir', '/tmp/flux-ipc-audit-target']),
    ('clippy', ['cargo', 'clippy', '--workspace', '--all-targets', '--locked', '--', '-D', 'warnings']),
    ('format', ['cargo', 'fmt', '--all', '--', '--check']),
    ('docs', ['python3', 'tools/check_docs.py']),
    ('whitespace', ['git', 'diff', '--check']),
]


def sources() -> dict[str, str]:
    paths = [ROOT / 'Cargo.toml', ROOT / 'Cargo.lock', ROOT / 'rust-toolchain.toml']
    paths.extend((ROOT / 'crates').rglob('*.rs'))
    paths.extend((ROOT / 'crates').rglob('Cargo.toml'))
    paths.extend((ROOT / 'tools').glob('*.py'))
    return {str(p.relative_to(ROOT)): hashlib.sha256(p.read_bytes()).hexdigest()
            for p in sorted(paths) if p.is_file()}


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', required=True, type=Path)
    args = parser.parse_args()
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=True)
    before = sources()
    results = []
    for name, argv in COMMANDS:
        start = time.monotonic()
        log = output / f'{name}.log'
        with log.open('wb') as stream:
            try:
                result = subprocess.run(argv, cwd=ROOT, stdout=stream,
                                        stderr=subprocess.STDOUT, timeout=180,
                                        env={**os.environ, 'CARGO_TERM_COLOR': 'never', 'NO_COLOR': '1'})
                status, exit_code = 'completed', result.returncode
            except subprocess.TimeoutExpired:
                status, exit_code = 'timeout', None
            except OSError as error:
                stream.write(str(error).encode())
                status, exit_code = 'launch-error', None
        results.append(dict(name=name, argv=argv, status=status, exit_code=exit_code,
                            elapsed_seconds=round(time.monotonic() - start, 3),
                            artifact=log.name, sha256=hashlib.sha256(log.read_bytes()).hexdigest()))
        print(f'{name}: {status}, exit={exit_code}', flush=True)
    after = sources()
    stable = before == after
    report = dict(timestamp_utc=datetime.now(timezone.utc).isoformat(),
                  scope='Local build/component/document checks; no compositor launch or native-device qualification',
                  source_files_sha256=before, source_unchanged_during_checks=stable,
                  results=results, passed=stable and all(r['exit_code'] == 0 for r in results),
                  limits=['No live protocol/global inventory', 'No compositor runtime or hardware tests',
                          'Existing unit tests can miss production defects',
                          'Native pkg-config metadata is not a full runtime dependency inventory'])
    (output / 'commands.json').write_text(json.dumps(report, indent=2) + '\n')
    return 0 if report['passed'] else 1


if __name__ == '__main__':
    sys.exit(main())
