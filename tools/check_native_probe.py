#!/usr/bin/env python3
"""Regression auditor for the trusted x86-64 Linux probe, not a general sandbox."""
import argparse
import ctypes
import errno
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import time

# Linux x86-64 ptrace ABI, not a portable syscall decoder. Unsupported hosts fail.
TRACEME, PEEKDATA, SYSCALL, GETREGS, SETREGS, SETOPTIONS = 0, 2, 24, 12, 13, 0x4200
TRACESYSGOOD, TRACEEXEC, EXITKILL = 1, 0x10, 0x100000
GET_SYSCALL_INFO = 0x420e
AUDIT_ARCH_X86_64 = 0xc000003e
MASK = 2**64 - 1
FIELDS = ('r15 r14 r13 r12 rbp rbx r11 r10 r9 r8 rax rcx rdx rsi rdi '
          'orig_rax rip cs eflags rsp ss fs_base gs_base ds es fs gs').split()


class Registers(ctypes.Structure):
    _fields_ = [(name, ctypes.c_ulonglong) for name in FIELDS]


class SyscallInfo(ctypes.Structure):
    # The entry member is the largest payload we read. Linux may report a
    # larger union size; ptrace copies only our supplied buffer length.
    _fields_ = [('op', ctypes.c_uint8), ('reserved', ctypes.c_uint8),
                ('flags', ctypes.c_uint16), ('arch', ctypes.c_uint32),
                ('ip', ctypes.c_uint64), ('sp', ctypes.c_uint64),
                ('payload', ctypes.c_uint64 * 7)]


LIBC = ctypes.CDLL(None, use_errno=True)
LIBC.ptrace.restype = ctypes.c_long
LIBC.ptrace.argtypes = [ctypes.c_ulong, ctypes.c_ulong, ctypes.c_void_p, ctypes.c_void_p]


def ptrace(request, pid, address=0, data=0):
    ctypes.set_errno(0)
    result = LIBC.ptrace(request, pid, address, data)
    error = ctypes.get_errno()
    if result == -1 and error:
        raise OSError(error, os.strerror(error))
    return result


def remote_string(pid, address):
    data = bytearray()
    for offset in range(0, 4096, 8):
        value = ptrace(PEEKDATA, pid, address + offset) & MASK
        block = value.to_bytes(8, 'little')
        if b'\0' in block:
            data.extend(block.split(b'\0', 1)[0])
            return os.fsdecode(bytes(data))
        data.extend(block)
    raise RuntimeError('open path exceeds audit string bound')


def open_path(pid, registers, number):
    if number in (2, 85):  # open / creat
        name = remote_string(pid, registers.rdi)
        directory = -100
    else:  # openat / openat2
        name = remote_string(pid, registers.rsi)
        directory = ctypes.c_longlong(registers.rdi).value
    for alias in ("/proc/self/", "/proc/thread-self/", "/dev/fd/"):
        if name.startswith(alias):
            prefix = "/proc/%s/fd/" % pid if alias == "/dev/fd/" else "/proc/%s/" % pid
            name = prefix + name[len(alias):]
            break
    if os.path.isabs(name):
        return os.path.realpath(name)
    base = os.readlink('/proc/%s/cwd' % pid) if directory == -100 else os.readlink('/proc/%s/fd/%s' % (pid, directory))
    return os.path.realpath(os.path.join(base, name))


def privileged(path):
    if not path.startswith('/dev/'):
        return False
    # Permit the harmless devices used by the loader/runtime; fail closed on
    # other device paths, including vendor GPUs and console/VT interfaces.
    return path not in {'/dev/null', '/dev/zero', '/dev/full',
                        '/dev/random', '/dev/urandom'}


def wait_owned(pid, deadline):
    while time.monotonic() < deadline:
        found, status = os.waitpid(pid, os.WNOHANG)
        if found:
            return status
        time.sleep(.0005)
    raise TimeoutError('owned trace child exceeded absolute deadline')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=Path('target/debug/fluxwm-native-probe'))
    parser.add_argument('--artifacts', type=Path, required=True)
    args = parser.parse_args()
    if platform.system() != 'Linux' or platform.machine() != 'x86_64':
        raise SystemExit('audit supports only the Linux x86-64 syscall ABI')
    directory = args.artifacts.resolve(); directory.mkdir(mode=0o700, parents=True)
    binary = args.binary.resolve()
    parent = dict(os.environ)
    command = [str(binary), '--check']
    result = {'ok': False, 'command': command, 'scope': 'owned ptrace child --check only; denies native device open/ioctl, connect, process/thread creation and later exec; no native session qualification',
              'general_security_sandbox': False,
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'harness_sha256': hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
              'syscall_stops': 0, 'checked_open_calls': 0, 'blocked_calls': []}
    parent_pid = os.getpid()
    pid = None
    reaped = False
    try:
        with (directory / 'stdout.json').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
            inherited = [int(p.name) for p in Path('/proc/self/fd').iterdir() if int(p.name) > 2]
            pid = os.fork()
            if pid == 0:
                try:
                    # Cover parent death before PTRACE_O_EXITKILL is installed.
                    if LIBC.prctl(1, signal.SIGKILL, 0, 0, 0) != 0:
                        os._exit(125)
                    if os.getppid() != parent_pid:
                        os._exit(125)
                    os.dup2(stdout.fileno(), 1); os.dup2(stderr.fileno(), 2)
                    null = os.open('/dev/null', os.O_RDONLY | os.O_CLOEXEC)
                    os.dup2(null, 0)
                    if null > 2:
                        os.close(null)
                    for fd in inherited:
                        try:
                            os.close(fd)
                        except OSError as error:
                            if error.errno != errno.EBADF:
                                raise
                    ptrace(TRACEME, 0)
                    os.kill(os.getpid(), signal.SIGSTOP)
                    environment = dict(parent, LIBSEAT_BACKEND='fluxwm-no-such-backend')
                    os.execve(binary, command, environment)
                except BaseException:
                    os._exit(125)
            result['owned_pid'] = pid
            deadline = time.monotonic() + 25
            status = wait_owned(pid, deadline)
            if not os.WIFSTOPPED(status):
                reaped = os.WIFEXITED(status) or os.WIFSIGNALED(status)
                raise RuntimeError('owned trace child failed before initial stop')
            ptrace(SETOPTIONS, pid, 0, TRACESYSGOOD | TRACEEXEC | EXITKILL)
            executed = False
            blocked = False
            ptrace(SYSCALL, pid)
            while True:
                status = wait_owned(pid, deadline)
                if os.WIFEXITED(status) or os.WIFSIGNALED(status):
                    reaped = True
                    result['exit_code'] = os.waitstatus_to_exitcode(status)
                    break
                stopped = os.WSTOPSIG(status)
                event = status >> 16
                if stopped == signal.SIGTRAP and event == 4:  # PTRACE_EVENT_EXEC
                    executed = True
                    ptrace(SYSCALL, pid)
                    continue
                if stopped != signal.SIGTRAP | 0x80:
                    # Deliver real signals, rather than silently hiding failures.
                    ptrace(SYSCALL, pid, 0, stopped)
                    continue
                result['syscall_stops'] += 1
                if result['syscall_stops'] > 40000:
                    raise RuntimeError('owned trace syscall budget exceeded')
                registers = Registers()
                ptrace(GETREGS, pid, 0, ctypes.byref(registers))
                info = SyscallInfo()
                size = ptrace(GET_SYSCALL_INFO, pid, ctypes.sizeof(info), ctypes.byref(info))
                if size < 24 or info.op not in (1, 2):
                    raise RuntimeError('missing kernel syscall entry/exit information')
                if info.arch != AUDIT_ARCH_X86_64:
                    raise RuntimeError('unsupported syscall ABI')
                if info.op == 1:
                    blocked = False
                    number = registers.orig_rax
                    reason = None
                    if executed:
                        if number & 0x40000000:
                            reason = 'unsupported-syscall-ABI'
                        elif number in (2, 85, 257, 437):
                            result['checked_open_calls'] += 1
                            path = open_path(pid, registers, number)
                            if privileged(path):
                                reason = 'native-device-open'
                        elif number in (303, 304):
                            reason = 'file-handle-access'
                        elif number == 42:
                            reason = 'connect'
                        elif number in (56, 57, 58, 435):
                            reason = 'process-or-thread-creation'
                        elif number in (59, 322):
                            reason = 'later-exec'
                        elif number == 16:
                            request_type = (registers.rsi >> 8) & 0xff
                            if request_type in (ord('d'), ord('E'), ord('K'), ord('V'), ord('F')):
                                reason = 'native-device-ioctl-family'
                            else:
                                try:
                                    target = os.readlink('/proc/%s/fd/%s' % (pid, registers.rdi))
                                except FileNotFoundError:
                                    target = ''
                                if privileged(target):
                                    reason = 'native-device-ioctl'
                        elif number in (425, 426, 427):
                            reason = 'io-uring'
                    if reason:
                        if len(result['blocked_calls']) >= 64:
                            raise RuntimeError('owned trace denial budget exceeded')
                        result['blocked_calls'].append({'syscall': number, 'reason': reason})
                        registers.orig_rax = MASK
                        ptrace(SETREGS, pid, 0, ctypes.byref(registers))
                        blocked = True
                elif blocked:
                    registers.rax = (-errno.EPERM) & MASK
                    ptrace(SETREGS, pid, 0, ctypes.byref(registers))
                ptrace(SYSCALL, pid)
        output = json.loads((directory / 'stdout.json').read_text())
        result['probe'] = output
        result['ok'] = (executed and result['exit_code'] == 0 and not result['blocked_calls'] and
                        output['session_opened'] is False and output['gpu_contexts_created'] == 0 and
                        output['native_compositor'] is False and result['checked_open_calls'] > 0)
    except Exception as error:
        result['error'] = str(error)
    finally:
        if pid and not reaped:
            # Revalidate ownership before signalling; ECHILD must never become
            # permission to signal a possibly reused PID.
            try:
                found, status = os.waitpid(pid, os.WNOHANG)
                if found and (os.WIFEXITED(status) or os.WIFSIGNALED(status)):
                    reaped = True
                else:
                    try:
                        os.kill(pid, signal.SIGKILL)
                    except ProcessLookupError:
                        pass
                    os.waitpid(pid, 0); reaped = True
            except ChildProcessError:
                result['ownership_lost'] = True
        result['child_reaped'] = reaped
        result['binary_unchanged'] = hashlib.sha256(binary.read_bytes()).hexdigest() == result['binary_sha256']
        result['harness_unchanged'] = hashlib.sha256(Path(__file__).read_bytes()).hexdigest() == result['harness_sha256']
        result['parent_environment_unchanged'] = dict(os.environ) == parent
        result['ok'] &= reaped and result['binary_unchanged'] and result['harness_unchanged'] and result['parent_environment_unchanged']
        (directory / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps({key: result.get(key) for key in ['ok','error','checked_open_calls','blocked_calls','child_reaped']}))
    return 0 if result['ok'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
