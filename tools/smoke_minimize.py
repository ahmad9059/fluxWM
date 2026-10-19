#!/usr/bin/env python3
"""Bounded real XDG minimize, restore, focus fallback and remap regression."""
import argparse
import json
import os
from pathlib import Path
import uuid

from smoke_health import run_case
from smoke_nested import sha256, write_json


def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--fixture', type=Path, default=root / 'target/debug/fluxwm-fixture')
    parser.add_argument('--artifacts', type=Path)
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-minimize' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    parent = dict(os.environ)
    case = run_case(args.binary.resolve(), args.fixture.resolve(), directory / 'minimize',
                    'minimize', parent, fixture_timeout_ms=5000)
    result = {'ok': case['ok'], 'cases': [case],
              'source_sha256': {name: sha256(root / 'tools' / name)
                                for name in ['smoke_minimize.py', 'smoke_health.py', 'smoke_nested.py']},
              'parent_environment_unchanged': parent == dict(os.environ),
              'scope': 'real XDG minimize and capabilities, focused IPC minimize/restore, hidden commits/frame suppression, focus fallback, minimize-all, stale ID and unmap/remap; excludes physical/native execution and pixel qualification'}
    result['ok'] = result['ok'] and result['parent_environment_unchanged']
    write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
