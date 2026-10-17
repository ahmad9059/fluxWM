#!/usr/bin/env python3
"""Real pointer/grab regression on an authenticated, owned private Xvfb only."""
from __future__ import annotations
import argparse
import ctypes as C
import hashlib
import json
import os
from pathlib import Path
import re
import secrets
import socket
import struct
import subprocess
import sys
import time
import urllib.request
import uuid

SIGNER = 'ADC8A1FCC15E01D45310419E94657AB20F2A092B'
MIRROR = 'https://geo.mirror.pkgbuild.com/extra/os/x86_64/'
KEYRING = Path('/usr/share/pacman/keyrings/archlinux.gpg')
REVOKED = Path('/usr/share/pacman/keyrings/archlinux-revoked')

class Failure(Exception):
    def __init__(self, code, message):
        super().__init__(message)
        self.code = code

def require(value, message, code='infrastructure'):
    if not value:
        raise Failure(code, message)

def digest(path):
    h = hashlib.sha256()
    with Path(path).open('rb') as source:
        for block in iter(lambda: source.read(1024 * 1024), b''):
            h.update(block)
    return h.hexdigest()

def write_json(path, value):
    Path(path).write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')

def run(command, timeout=10, **kwargs):
    return subprocess.run(command, timeout=timeout, check=True, **kwargs)

def download(url, path, maximum):
    start = time.monotonic()
    total = 0
    with urllib.request.urlopen(url, timeout=12) as response, path.open('xb') as target:
        require(response.geturl().startswith('https://'), 'download redirected outside HTTPS')
        while block := response.read(128 * 1024):
            total += len(block)
            require(total <= maximum and time.monotonic() - start < 30, 'download budget exceeded')
            target.write(block)

def prepare_xvfb(bundle, directory):
    """Never execute extracted code before hash AND pinned trusted signature pass."""
    if bundle is None:
        bundle = directory / 'package'
        bundle.mkdir(mode=0o700)
        download('https://archlinux.org/packages/extra/x86_64/xorg-server-xvfb/json/', bundle / 'official.json', 128 * 1024)
        metadata = json.loads((bundle / 'official.json').read_text())
        require((metadata['pkgname'], metadata['repo'], metadata['arch']) == ('xorg-server-xvfb', 'extra', 'x86_64'), 'unexpected official package identity')
        filename = metadata['filename']
        require(re.fullmatch(r'xorg-server-xvfb-[0-9A-Za-z.+_-]+-x86_64\.pkg\.tar\.zst', filename), 'unsafe package filename')
        download(MIRROR + filename, bundle / filename, 16 * 1024 * 1024)
        download(MIRROR + filename + '.sig', bundle / (filename + '.sig'), 64 * 1024)
        download(MIRROR + 'extra.db', bundle / 'extra.db', 32 * 1024 * 1024)
    files = list(bundle.glob('xorg-server-xvfb-*-x86_64.pkg.tar.zst'))
    require(len(files) == 1, 'bundle must contain exactly one official Xvfb package')
    package = files[0]
    require(re.fullmatch(r'xorg-server-xvfb-[0-9A-Za-z.+_-]+-x86_64\.pkg\.tar\.zst', package.name), 'unsafe package filename')
    entry = package.name.removesuffix('-x86_64.pkg.tar.zst') + '/desc'
    desc = run(['bsdtar', '-xOf', str(bundle / 'extra.db'), entry], capture_output=True).stdout.decode()
    fields = {lines[0].strip('%'): '\n'.join(lines[1:]) for block in desc.strip().split('\n\n') if (lines := block.splitlines())}
    require(fields['NAME'] == 'xorg-server-xvfb' and fields['ARCH'] == 'x86_64' and fields['FILENAME'] == package.name, 'repository package metadata mismatch')
    sha = digest(package)
    require(sha == fields['SHA256SUM'], 'package SHA256 differs from repository metadata')
    require(SIGNER not in REVOKED.read_text().splitlines(), 'pinned package signer is revoked')
    keyring = directory / 'archlinux-binary.gpg'
    run(['gpg', '--homedir', str(directory), '--batch', '--dearmor', '--output', str(keyring), str(KEYRING)], capture_output=True)
    verification = run(['gpgv', '--homedir', str(directory), '--keyring', str(keyring), '--status-fd', '1', str(package) + '.sig', str(package)], capture_output=True)
    status = verification.stdout.decode()
    require('[GNUPG:] VALIDSIG ' + SIGNER + ' ' in status, 'signature signer differs from reviewed official packager')
    require(not any(word in status for word in ('REVKEYSIG', 'EXPKEYSIG', 'EXPSIG', 'BADSIG')), 'signature status is not acceptable')
    pkginfo = run(['bsdtar', '-xOf', str(package), '.PKGINFO'], capture_output=True).stdout.decode()
    require('pkgname = xorg-server-xvfb\n' in pkginfo and 'arch = x86_64\n' in pkginfo, 'signed package identity differs')
    executable = directory / 'Xvfb'
    with executable.open('xb') as output:
        run(['bsdtar', '-xOf', str(package), 'usr/bin/Xvfb'], stdout=output)
    executable.chmod(0o700)
    evidence = {'filename': package.name, 'sha256': sha, 'repository_sha256': fields['SHA256SUM'], 'signer': SIGNER,
                'signature_verified': True, 'keyring': str(KEYRING), 'keyring_sha256': digest(KEYRING),
                'metadata_url': 'https://archlinux.org/packages/extra/x86_64/xorg-server-xvfb/json/', 'repository_url': MIRROR + 'extra.db', 'package_url': MIRROR + package.name,
                'signature_status': status, 'executable_sha256': digest(executable), 'bundle': str(bundle)}
    write_json(directory / 'package-verification.json', evidence)
    return executable, evidence

class PrivateX:
    """Only instantiated in a deadline-bounded helper process; no default display."""
    def __init__(self, request):
        self.request = request
        number = request['display_number']
        require(isinstance(number, int) and 4000 <= number < 20000, 'refusing nonprivate display range')
        require(Path(f'/tmp/.X{number}-lock').read_text().strip() == str(request['server_pid']), 'X display does not belong to owned Xvfb PID')
        require(os.getpgid(request['server_pid']) == request['server_pid'], 'X server is not the owned private session leader')
        auth = Path(request['auth_file'])
        require(auth.stat().st_mode & 0o077 == 0 and auth.stat().st_uid == os.geteuid(), 'unsafe private authority permissions')
        raw = auth.read_bytes()
        require(len(raw) < 1024, 'oversized private authority')
        offset = 2
        pieces = []
        for _ in range(4):
            size, = struct.unpack_from('>H', raw, offset); offset += 2
            pieces.append(raw[offset:offset + size]); offset += size
        require(offset == len(raw) and pieces[2] == b'MIT-MAGIC-COOKIE-1' and len(pieces[3]) == 16, 'invalid private authority')
        self.x = C.CDLL('libX11.so.6')
        self.t = C.CDLL('libXtst.so.6')
        signatures = {
            'XSetAuthorization': ([C.c_char_p, C.c_int, C.c_char_p, C.c_int], None),
            'XOpenDisplay': ([C.c_char_p], C.c_void_p), 'XCloseDisplay': ([C.c_void_p], C.c_int),
            'XDefaultRootWindow': ([C.c_void_p], C.c_ulong),
            'XInternAtom': ([C.c_void_p, C.c_char_p, C.c_int], C.c_ulong),
            'XGetWindowProperty': ([C.c_void_p, C.c_ulong, C.c_ulong, C.c_long, C.c_long, C.c_int, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ubyte))], C.c_int),
            'XQueryTree': ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_ulong), C.POINTER(C.POINTER(C.c_ulong)), C.POINTER(C.c_uint)], C.c_int),
            'XFree': ([C.c_void_p], C.c_int), 'XFetchName': ([C.c_void_p, C.c_ulong, C.POINTER(C.c_char_p)], C.c_int),
            'XGetGeometry': ([C.c_void_p, C.c_ulong, C.POINTER(C.c_ulong), C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_uint), C.POINTER(C.c_uint), C.POINTER(C.c_uint), C.POINTER(C.c_uint)], C.c_int),
            'XTranslateCoordinates': ([C.c_void_p, C.c_ulong, C.c_ulong, C.c_int, C.c_int, C.POINTER(C.c_int), C.POINTER(C.c_int), C.POINTER(C.c_ulong)], C.c_int),
            'XSetInputFocus': ([C.c_void_p, C.c_ulong, C.c_int, C.c_ulong], C.c_int), 'XRaiseWindow': ([C.c_void_p, C.c_ulong], C.c_int),
            'XSync': ([C.c_void_p, C.c_int], C.c_int),
        }
        for name, (arguments, result) in signatures.items():
            function = getattr(self.x, name); function.argtypes = arguments; function.restype = result
        for name, arguments in [('XTestQueryExtension', [C.c_void_p] + [C.POINTER(C.c_int)] * 4), ('XTestFakeMotionEvent', [C.c_void_p, C.c_int, C.c_int, C.c_int, C.c_ulong]), ('XTestFakeButtonEvent', [C.c_void_p, C.c_uint, C.c_int, C.c_ulong])]:
            function = getattr(self.t, name); function.argtypes = arguments; function.restype = C.c_int
        self.x.XSetAuthorization(pieces[2], len(pieces[2]), pieces[3], len(pieces[3]))
        self.d = self.x.XOpenDisplay(f':{number}'.encode())
        require(self.d, 'could not authenticate to private Xvfb')
        self.root = self.x.XDefaultRootWindow(self.d)
        event, error, major, minor = (C.c_int() for _ in range(4))
        require(self.t.XTestQueryExtension(self.d, C.byref(event), C.byref(error), C.byref(major), C.byref(minor)), 'private X server has no XTEST extension')
    def owner(self, window):
        atom = self.x.XInternAtom(self.d, b'_NET_WM_PID', 1)
        actual, fmt, count, after = C.c_ulong(), C.c_int(), C.c_ulong(), C.c_ulong()
        data = C.POINTER(C.c_ubyte)()
        status = self.x.XGetWindowProperty(self.d, window, atom, 0, 1, 0, 6, C.byref(actual), C.byref(fmt), C.byref(count), C.byref(after), C.byref(data))
        try:
            return C.cast(data, C.POINTER(C.c_ulong))[0] if status == 0 and fmt.value == 32 and count.value == 1 and data else None
        finally:
            if data: self.x.XFree(data)
    def target(self):
        root, parent, children, count = C.c_ulong(), C.c_ulong(), C.POINTER(C.c_ulong)(), C.c_uint()
        require(self.x.XQueryTree(self.d, self.root, C.byref(root), C.byref(parent), C.byref(children), C.byref(count)), 'private root query failed')
        matches = []
        try:
            require(count.value <= 4096, 'private window count exceeds bound')
            for index in range(count.value):
                window = children[index]
                if self.owner(window) != self.request['target_pid']: continue
                name = C.c_char_p()
                if self.x.XFetchName(self.d, window, C.byref(name)) and name:
                    try:
                        if name.value.decode() == 'FluxWM (nested)': matches.append(window)
                    finally: self.x.XFree(name)
        finally:
            if children: self.x.XFree(children)
        require(len(matches) == 1, 'exact owned FluxWM X window not found', 'target_not_ready')
        return matches[0]
    def geometry(self, window):
        root, x, y, w, h, border, depth = C.c_ulong(), C.c_int(), C.c_int(), C.c_uint(), C.c_uint(), C.c_uint(), C.c_uint()
        require(self.x.XGetGeometry(self.d, window, C.byref(root), C.byref(x), C.byref(y), C.byref(w), C.byref(h), C.byref(border), C.byref(depth)), 'target geometry unavailable')
        require(320 <= w.value <= 1600 and 240 <= h.value <= 1000, 'target size outside private screen bounds')
        return {'width': w.value, 'height': h.value}
    def inject(self, window, action):
        self.x.XRaiseWindow(self.d, window)
        self.x.XSetInputFocus(self.d, window, 1, 0)
        if action['kind'] == 'motion':
            geometry = self.geometry(window); x, y = action['x'], action['y']
            require(0 <= x < geometry['width'] and 0 <= y < geometry['height'], 'motion outside exact owned target')
            rx, ry, child = C.c_int(), C.c_int(), C.c_ulong()
            require(self.x.XTranslateCoordinates(self.d, window, self.root, x, y, C.byref(rx), C.byref(ry), C.byref(child)), 'target coordinate conversion failed')
            require(self.t.XTestFakeMotionEvent(self.d, 0, rx.value, ry.value, 0), 'XTEST motion failed')
        else:
            require(action['button'] in (1, 4, 5, 6, 7), 'unsupported private test button')
            require(self.t.XTestFakeButtonEvent(self.d, action['button'], int(action['pressed']), 0), 'XTEST button failed')
        self.x.XSync(self.d, 0)
    def close(self):
        self.x.XCloseDisplay(self.d)

def xtest_helper():
    request = json.load(sys.stdin)
    connection = PrivateX(request)
    try:
        target = connection.target()
        geometry = connection.geometry(target)
        if request.get('action'):
            connection.inject(target, request['action'])
        print(json.dumps({'ok': True, 'window': target, 'pid': request['target_pid'], **geometry}))
    finally:
        connection.close()

class Session:
    def __init__(self, executable, directory, screen="1600x1000x24"):
        require(screen in ("1600x1000x24", "3000x2000x24"), 'unsupported private screen size')
        self.directory = directory
        self.parent = dict(os.environ)
        self.number = None
        for _ in range(100):
            number = 4000 + secrets.randbelow(16000)
            if not Path(f'/tmp/.X11-unix/X{number}').exists() and not Path(f'/tmp/.X{number}-lock').exists() and self.parent.get('DISPLAY') not in (f':{number}', f':{number}.0'):
                self.number = number
                break
        require(self.number is not None, 'no unused private display number found')
        self.display = f':{self.number}'
        self.auth = directory / 'private.Xauthority'
        cookie = secrets.token_bytes(16)
        record = struct.pack('>H', 65535)
        for value in (b'', str(self.number).encode(), b'MIT-MAGIC-COOKIE-1', cookie): record += struct.pack('>H', len(value)) + value
        self.log = self.server = None
        try:
            with os.fdopen(os.open(self.auth, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600), 'wb') as target: target.write(record)
            self.log = (directory / 'xvfb.log').open('wb')
            self.server = subprocess.Popen([str(executable), self.display, '-screen', '0', screen, '-nolisten', 'tcp', '-auth', str(self.auth), '-noreset'], stdout=self.log, stderr=self.log, start_new_session=True)
        except BaseException:
            if self.log is not None: self.log.close()
            self.auth.unlink(missing_ok=True)
            raise
        self.environment = dict(self.parent, DISPLAY=self.display, XAUTHORITY=str(self.auth), WINIT_UNIX_BACKEND='x11', LIBGL_ALWAYS_SOFTWARE='1')
        self.environment.pop('WAYLAND_DISPLAY', None); self.environment.pop('WAYLAND_SOCKET', None)
    def ready(self):
        return self.server.poll() is None and Path(f'/tmp/.X{self.number}-lock').exists() and Path(f'/tmp/.X11-unix/X{self.number}').exists()
    def test(self, pid, action=None, timeout=2):
        require(self.server.poll() is None, 'owned Xvfb exited')
        request = {'display_number': self.number, 'server_pid': self.server.pid, 'auth_file': str(self.auth), 'target_pid': pid, 'action': action}
        # Cookie stays in the 0600 file, never argv/evidence. Native Xlib calls
        # are isolated here so a hung X call is killed by the parent deadline.
        completed = subprocess.run([sys.executable, str(Path(__file__).resolve()), '_xtest'], input=json.dumps(request), capture_output=True, text=True, timeout=timeout, env=self.environment)
        require(completed.returncode == 0, 'private XTEST helper failed: ' + completed.stderr[-2000:])
        return json.loads(completed.stdout)
    def close(self):
        if self.server.poll() is None:
            self.server.terminate()
            try: self.server.wait(timeout=2)
            except subprocess.TimeoutExpired: self.server.kill(); self.server.wait(timeout=2)
        self.log.close()
        # Secrets have no role in evidence reproduction. Remove cookie after all
        # owned X clients/server have stopped; retain only path/mode metadata.
        self.auth.unlink(missing_ok=True)

def main():
    root = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', type=Path, default=root / 'target/debug/fluxwm')
    parser.add_argument('--fixture', type=Path, default=root / 'target/debug/fluxwm-fixture')
    parser.add_argument('--bundle', type=Path, help='cached official package + detached signature + matching extra.db; all reverified')
    parser.add_argument('--artifacts', type=Path)
    parser.add_argument('--prepare-only', action='store_true')
    parser.add_argument('--case', choices=('scene-pan', 'held-motion'), default='scene-pan')
    args = parser.parse_args()
    directory = (args.artifacts or root / 'target/smoke-input' / uuid.uuid4().hex).resolve()
    directory.mkdir(mode=0o700, parents=True, exist_ok=False)
    result = {'ok': False, 'case': args.case, 'harness_sha256': digest(Path(__file__)), 'invocation': [sys.executable, *sys.argv], 'scope': 'private authenticated X11 host -> nested FluxWM -> owned Wayland input-probe; no host-session injection'}
    session = compositor = fixture = None
    logs = []
    started = time.monotonic()
    deadline = None
    responses = []
    def check():
        require(time.monotonic() < deadline, 'runtime suite deadline exceeded')
        require(compositor is None or compositor.poll() is None, 'nested compositor exited early')
        require(session.server.poll() is None, 'private Xvfb exited early')
    def wait(predicate, maximum=3):
        stop = min(deadline, time.monotonic() + maximum)
        while not predicate():
            check(); require(time.monotonic() < stop, 'event observation deadline exceeded')
            time.sleep(.01)
    def events():
        if not event_file.exists(): return []
        require(event_file.stat().st_size <= 1024 * 1024, 'fixture event evidence too large')
        text = event_file.read_text()
        lines = text.splitlines()
        if text and not text.endswith('\n'):
            lines = lines[:-1] # A final append may still be in progress.
        data = [json.loads(line) for line in lines]
        require(len(data) <= 1024, 'fixture event count exceeds bound')
        return data
    def observed(kind, owner=None, since=0, **values):
        return any(e.get('type') == kind and (owner is None or e.get('root') == owner) and all(e.get(k) == v for k, v in values.items()) for record in events()[since:] if (e := record.get('event', {})))
    def ipc(method, params=None):
        check(); ident = uuid.uuid4().hex
        message = json.dumps({'version': 1, 'id': ident, 'method': method, 'params': params or {}}).encode()
        with socket.socket(socket.AF_UNIX) as connection:
            connection.settimeout(min(2, max(.01, deadline - time.monotonic())))
            connection.connect(str(ipc_path)); connection.sendall(struct.pack('>I', len(message)) + message)
            def exact(count):
                data = b''
                while len(data) < count:
                    check(); part = connection.recv(count - len(data)); require(part, 'unexpected IPC EOF'); data += part
                return data
            size, = struct.unpack('>I', exact(4)); require(0 < size <= 1024 * 1024, 'invalid reply length')
            response = json.loads(exact(size)); require(response['id'] == ident and 'result' in response, 'IPC rejected test operation')
        responses.append(response); return response['result']
    def action(name, **values): return ipc('action', {'name': name, **values})
    def inject(kind, **values):
        check(); return session.test(compositor.pid, {'kind': kind, **values}, timeout=min(2, max(.001, deadline - time.monotonic())))
    try:
        executable, verification = prepare_xvfb(args.bundle, directory)
        result['package'] = verification
        if args.prepare_only:
            result['ok'] = True
        else:
            # Copy stable executable identities before launching any process.
            for source, name in [(args.binary.resolve(), 'fluxwm'), (args.fixture.resolve(), 'fixture')]:
                before = digest(source)
                import shutil
                shutil.copyfile(source, directory / name); (directory / name).chmod(0o700)
                require(digest(source) == before == digest(directory / name), 'binary changed while taking evidence snapshot')
                result[name + '_sha256'] = before
            session = Session(executable, directory)
            deadline = time.monotonic() + 30
            wait(session.ready)
            runtime = directory / 'runtime'; runtime.mkdir(mode=0o700)
            environment = dict(session.environment, XDG_RUNTIME_DIR=str(runtime))
            name = 'fluxwm-input-' + uuid.uuid4().hex[:12]
            ready = directory / 'ready.json'
            for log_name in ('compositor.stdout', 'compositor.stderr', 'fixture.stdout', 'fixture.stderr'): logs.append((directory / log_name).open('wb'))
            compositor = subprocess.Popen([str(directory / 'fluxwm'), '--socket', name, '--ready-file', str(ready), '--exit-after-ms', '18000'], env=environment, stdout=logs[0], stderr=logs[1], start_new_session=True)
            wait(ready.exists, 8)
            publication = json.loads(ready.read_text()); require(publication['pid'] == compositor.pid, 'ready process mismatch'); ipc_path = Path(publication['ipc_socket'])
            result['private_display'] = {'display': session.display, 'server_pid': session.server.pid, 'authenticated': True, 'auth_path': str(session.auth), 'auth_mode': '0600', 'tcp_disabled': True}
            result['target'] = session.test(compositor.pid)
            event_file = directory / 'input.events.jsonl'
            fixture_environment = dict(environment, WAYLAND_DISPLAY=name, FLUXWM_SOCKET=str(ipc_path), FLUXWM_FIXTURE_EVENT_FILE=str(event_file))
            fixture = subprocess.Popen([str(directory / 'fixture'), 'input-probe', '--timeout-ms', '10000'], env=fixture_environment, stdout=logs[2], stderr=logs[3], start_new_session=True)
            wait(lambda: observed('ready'), 8)
            windows = ipc('windows.list')['windows']
            owners = {w['title']: w for w in windows}
            require(set(owners) == {'input-A', 'input-B'}, 'isolated fixture window inventory differs')
            if not owners['input-A']['focused']: action('focus', direction='left')
            action('workspace-mode', mode='canvas')
            windows = {w['title']: w for w in ipc('windows.list')['windows']}
            delta = windows['input-B']['geometry']['x'] - windows['input-A']['geometry']['x']
            require(100 < delta < 1200, 'fixture horizontal displacement outside controlled test range')
            inject('motion', x=64, y=64)
            wait(lambda: observed('pointer.enter', 0))
            before_press_scene = ipc('diagnostics')['runtime']['compositions']
            result['before_press_scene'] = before_press_scene
            press = len(events())
            inject('button', button=1, pressed=True)
            wait(lambda: observed('pointer.button', 0, button=272, state='pressed'))
            if args.case == 'scene-pan':
                before_pan = ipc('diagnostics')['runtime']['compositions']
                action('pan', dx=delta, dy=0)
                wait(lambda: ipc('diagnostics')['runtime']['compositions'] > before_pan)
                target_x = 64
            else:
                # Keep the scene fixed. Motion updates the pending target during
                # the implicit grab; release must repair it without later motion.
                moved = len(events())
                target_x = int(delta + 64)
                require(target_x == delta + 64, 'fixture displacement is not an exact test pixel')
                inject('motion', x=target_x, y=64)
                wait(lambda: observed('pointer.motion', 0, since=moved))
                time.sleep(.2)
                check()
                result['held_motion_scene'] = ipc('diagnostics')['runtime']['compositions']
            if args.case == 'held-motion':
                require(result['held_motion_scene'] == before_press_scene, 'composition during held motion invalidates release-specific oracle', 'oracle_composition_changed')
            held = len(events())
            inject('button', button=4, pressed=True); inject('button', button=4, pressed=False)
            wait(lambda: observed('pointer.axis', 0, since=held, axis='vertical'))
            require(not observed('pointer.enter', 1, since=press), 'implicit grab retargeted before button release', 'grab_routing')
            release = len(events())
            result['release_event_index'] = release
            inject('button', button=1, pressed=False)
            wait(lambda: observed('pointer.button', 0, since=release, button=272, state='released'))
            stop = time.monotonic() + 1
            while not observed('pointer.enter', 1, since=release) and time.monotonic() < stop:
                check(); time.sleep(.01)
            if args.case == 'held-motion':
                after_enter_scene = ipc('diagnostics')['runtime']['compositions']
                result['composition_oracle'] = {'before_press': before_press_scene, 'held': result['held_motion_scene'], 'after_observation': after_enter_scene}
                require(before_press_scene == after_enter_scene, 'composition during observation invalidates release-specific oracle', 'oracle_composition_changed')
            require(observed('pointer.enter', 1, since=release), 'stationary button release did not restore pending focus to input-B', 'M1_missing_focus_after_release')
            journal = events()
            def event_index(kind, owner, **values):
                return next(i for i, record in enumerate(journal[release:], release)
                            if (e := record.get('event', {})).get('type') == kind
                            and e.get('root') == owner and all(e.get(k) == v for k, v in values.items()))
            release_i = event_index('pointer.button', 0, button=272, state='released')
            leave_i = event_index('pointer.leave', 0)
            enter_i = event_index('pointer.enter', 1)
            require(release_i < leave_i < enter_i, 'release/leave/enter journal order differs', 'oracle_event_order')
            result['release_order'] = {'release': release_i, 'leave': leave_i, 'enter': enter_i}
            after_release = len(events())
            inject('button', button=4, pressed=True); inject('button', button=4, pressed=False)
            wait(lambda: observed('pointer.axis', 1, since=after_release, axis='vertical'))
            inject('motion', x=target_x + 2, y=67)
            wait(lambda: observed('pointer.motion', 1, since=after_release))
            inject('button', button=1, pressed=True); inject('button', button=1, pressed=False)
            wait(lambda: observed('pointer.button', 1, since=after_release, button=272, state='released'))
            wait(lambda: fixture.poll() is not None, 9)
            require(fixture.returncode == 0, 'input fixture reported an instrumentation/protocol failure')
            fixture_report = json.loads((directory / 'fixture.stdout').read_text())
            require(fixture_report.get('ok') is True, 'fixture terminal report is not successful')
            result['fixture_report'] = fixture_report
            result.update({'ok': True, 'M1_pending_focus_repaired_without_motion': True, 'held_axis_owner': 0, 'released_axis_owner': 1, 'fixture_horizontal_delta': delta, 'pan_dx': delta if args.case == 'scene-pan' else 0, 'release_event_index': release})
    except Exception as error:
        result.update({'error': str(error), 'failure_code': getattr(error, 'code', 'infrastructure')})
    finally:
        if session is not None:
            # Release any test-held button on the owned private server before teardown.
            if compositor is not None and compositor.poll() is None:
                try: session.test(compositor.pid, {'kind': 'button', 'button': 1, 'pressed': False})
                except Exception: pass
        for process in (fixture, compositor):
            if process is not None and process.poll() is None:
                process.terminate()
                try: process.wait(timeout=2)
                except subprocess.TimeoutExpired: process.kill(); process.wait(timeout=2)
        if session is not None: session.close()
        for log in logs: log.close()
        result['elapsed_seconds'] = time.monotonic() - started
        result['responses'] = responses
        result['parent_environment_unchanged'] = dict(os.environ) == (session.parent if session else dict(os.environ))
        write_json(directory / 'result.json', result)
    print(json.dumps({'ok': result['ok'], 'failure_code': result.get('failure_code'), 'error': result.get('error'), 'artifacts': str(directory)}))
    return 0 if result['ok'] else 1

if __name__ == '__main__':
    if sys.argv[1:] == ['_xtest']:
        try: xtest_helper()
        except Exception as error:
            print(str(error), file=sys.stderr); sys.exit(2)
    else:
        sys.exit(main())
