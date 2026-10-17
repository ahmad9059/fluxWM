#!/usr/bin/env python3
"""Bounded configure stream, latest-state recovery and remap lifecycle fixture."""
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
    directory = (args.artifacts or root / 'target/smoke-configure' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    parent = dict(os.environ)
    # Reuse the bounded process/IPC/readiness/cleanup runner. The fixture owns
    # protocol oracles; the runner also verifies shell monitoring retires.
    cases = [run_case(args.binary.resolve(), args.fixture.resolve(), directory / mode, mode, parent, fixture_timeout_ms=5000)
             for mode in ['configure-stream', 'configure-admission']]
    result = {'ok': all(case['ok'] for case in cases), 'cases': cases,
              'source_sha256': {name: sha256(root / 'tools' / name) for name in ['smoke_configure.py', 'smoke_health.py', 'smoke_nested.py']},
              'parent_environment_unchanged': parent == dict(os.environ),
              'scope': 'one non-acking mapped SHM client, healthy sibling, latest-state ack/commit, two remap orderings role replacement and rejected-admission recovery; no full transaction/native/global resource qualification'}
    write_json(directory / 'summary.json', result)
    print(json.dumps({'ok': result['ok'], 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
