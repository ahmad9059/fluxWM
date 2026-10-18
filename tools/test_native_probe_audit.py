#!/usr/bin/env python3
"""Exercise audit denials on owned, harmless syscall fixtures; no seat is opened."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess

SOURCE = r'''
#include <fcntl.h>
#include <stdio.h>
#include <string.h>
#include <sys/ioctl.h>
#include <sys/syscall.h>
#include <unistd.h>
int main(int argc, char **argv) {
    (void)argc;
    if (strstr(argv[0], "creat"))
        syscall(SYS_creat, "/dev/nvidia-fluxwm-audit-missing", 0600);
    if (strstr(argv[0], "vendor"))
        syscall(SYS_openat, AT_FDCWD, "/dev/nvidia-fluxwm-audit-missing", O_RDONLY);
    if (strstr(argv[0], "console"))
        syscall(SYS_openat, AT_FDCWD, "/dev/console-fluxwm-audit-missing", O_RDONLY);
    if (strstr(argv[0], "ioctl")) {
        int fd = open("/dev/null", O_RDONLY);
        ioctl(fd, 0x4b3a, 0); /* KDSETMODE family, harmless /dev/null fixture */
        close(fd);
    }
    if (strstr(argv[0], "handle"))
        syscall(SYS_open_by_handle_at, -1, NULL, 0);
    if (strstr(argv[0], "compat")) {
        long result;
        __asm__ volatile("int $0x80" : "=a"(result) : "0"(20) : "memory"); /* i386 getpid */
    }
    puts("{\"session_opened\":false,\"gpu_contexts_created\":0,\"native_compositor\":false}");
    return 0;
}
'''


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--artifacts', type=Path, required=True)
    args = parser.parse_args()
    out = args.artifacts.resolve()
    out.mkdir(parents=True)
    source = out / 'fixture.c'
    source.write_text(SOURCE)
    binary = out / 'normal'
    subprocess.run(['cc', '-Wall', '-Wextra', '-Werror', str(source), '-o', str(binary)], check=True)
    rows = []
    for case, reason in [('normal', None), ('creat', 'native-device-open'),
                         ('vendor', 'native-device-open'), ('console', 'native-device-open'),
                         ('ioctl', 'native-device-ioctl-family'), ('handle', 'file-handle-access'),
                         ('compat', 'unsupported syscall ABI')]:
        executable = out / case
        if case != 'normal':
            os.link(binary, executable)
        command = ['python3', 'tools/check_native_probe.py', '--binary', str(executable),
                   '--artifacts', str(out / (case + '-audit'))]
        completed = subprocess.run(command, capture_output=True, timeout=35)
        (out / (case + '.log')).write_bytes(completed.stdout + completed.stderr)
        result = json.loads((out / (case + '-audit/result.json')).read_text())
        observed = ([item['reason'] for item in result['blocked_calls']] + [result.get('error')])
        ok = (completed.returncode == (1 if reason else 0) and result['child_reaped'] and
              result['binary_unchanged'] and result['harness_unchanged'] and
              (reason in observed if reason else result['ok']))
        rows.append(dict(case=case, expected=reason, observed=observed, ok=ok,
                         command=command, exit_code=completed.returncode))
    (out / 'results.json').write_text(json.dumps(dict(
        source_sha256=hashlib.sha256(source.read_bytes()).hexdigest(),
        binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(), cases=rows), indent=2) + '\n')
    print(json.dumps(dict(ok=all(row['ok'] for row in rows), cases=len(rows))))
    return 0 if all(row['ok'] for row in rows) else 1


if __name__ == '__main__':
    raise SystemExit(main())
