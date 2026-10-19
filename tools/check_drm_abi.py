#!/usr/bin/env python3
"""Compare pinned Rust DRM layouts/opcode to local C headers; never open a GPU."""
import argparse
import hashlib
import json
from pathlib import Path
import shlex
import subprocess

SOURCE = r'''
#include <stdio.h>
#include <stddef.h>
#include <drm.h>
#include <drm_mode.h>
int main(void) {
    printf("{\"atomic_size\":%zu,\"atomic_cookie_offset\":%zu,\"event_size\":%zu,\"event_cookie_offset\":%zu,\"event_crtc_offset\":%zu,\"atomic_ioctl\":%lu,\"test_flags\":%u}\n",
        sizeof(struct drm_mode_atomic), offsetof(struct drm_mode_atomic,user_data),
        sizeof(struct drm_event_vblank), offsetof(struct drm_event_vblank,user_data),
        offsetof(struct drm_event_vblank,crtc_id), (unsigned long)DRM_IOCTL_MODE_ATOMIC,
        (unsigned)(DRM_MODE_ATOMIC_TEST_ONLY | DRM_MODE_ATOMIC_ALLOW_MODESET));
    return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    out = parser.parse_args().artifacts.resolve()
    out.mkdir(parents=True)
    rows = []

    def run(name, command):
        completed = subprocess.run(command, capture_output=True, timeout=180)
        (out / (name + '.stdout')).write_bytes(completed.stdout)
        (out / (name + '.stderr')).write_bytes(completed.stderr)
        rows.append(dict(command=command, exit_code=completed.returncode))
        (out / 'commands.json').write_text(json.dumps(rows, indent=2) + '\n')
        completed.check_returncode()
        return completed.stdout.decode()

    source = out / 'layout.c'
    source.write_text(SOURCE)
    cflags = shlex.split(run('pkg-config', ['pkg-config', '--cflags', 'libdrm']))
    run('compile', ['cc', '-Wall', '-Wextra', '-Werror', *cflags, str(source), '-o', str(out / 'layout')])
    native = json.loads(run('c-layout', [str(out / 'layout')]))
    output = run('rust-layout', ['cargo', 'test', '-p', 'flux-native', '--features', 'system', '--locked',
                                'system::atomic::tests::uapi_layout_matches_current_linux_struct_contract',
                                '--', '--exact', '--nocapture'])
    records = [line.split('fluxwm-drm-abi:', 1)[1] for line in output.splitlines() if 'fluxwm-drm-abi:' in line]
    assert len(records) == 1, 'expected exactly one executed Rust layout record'
    rust = json.loads(records[0])
    result = dict(ok=rust == native, c=native, rust=rust,
                  source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
                  harness_sha256=hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
                  scope='Local host C/Rust ABI comparison only; no DRM device opened or ioctl issued')
    (out / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
