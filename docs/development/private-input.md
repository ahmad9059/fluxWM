# Private nested pointer and grab regression

`tools/smoke_input.py` sends real XTEST pointer input through an owned private Xvfb,
the nested winit backend, and the fixture's Wayland `wl_pointer`. It checks client
observations instead of inferring delivery from successful injection. It requires
the `input-probe` fixture mode introduced in `a04c5ae`.

Run from the repository after building the compositor and fixture:

```sh
python3 tools/smoke_input.py --case scene-pan
python3 tools/smoke_input.py --case held-motion
```

Each invocation creates a new artifact directory under `target/smoke-input`.
`--artifacts PATH` chooses a new, nonexistent directory. `--binary PATH` and
`--fixture PATH` select executables; the harness copies and hashes them before
launch. Exit zero means the specified case passed, including successful normal
fixture completion. Exit one means an invariant or prerequisite failed; inspect
`result.json` for `failure_code` and the error. An infrastructure failure is not
a compositor regression result.

The first invocation downloads the official Arch `xorg-server-xvfb` package,
detached signature, and matching `extra.db`. Prerequisites are Linux x86_64, Python
3, `bsdtar`, `gpg`, `gpgv`, the installed Arch keyring and revocation list,
`libX11.so.6`, `libXtst.so.6`, and the executable's dynamic dependencies.
`--prepare-only` verifies/extracts without launching a display. `--bundle PATH`
reuses a directory containing exactly one package, its `.sig`, and matching
`extra.db`; every run repeats verification. No system package installation occurs.

The package's name, architecture, filename and SHA256 must agree with repository
metadata. The detached signature must verify against the installed Arch keyring
and the pinned Andreas Radke fingerprint
`ADC8A1FCC15E01D45310419E94657AB20F2A092B`; revoked/expired/bad signatures fail.
Signed `.PKGINFO` must confirm the package identity. Only `usr/bin/Xvfb` is
extracted. A packager/key change needs explicit review before updating the pin.
Repository metadata provides the hash cross-check; the pinned package signature
provides authentication. See the [official package page](https://archlinux.org/packages/extra/x86_64/xorg-server-xvfb/)
and [XTEST specification](https://xorg.freedesktop.org/archive/current/doc/libXtst/xtestlib.html).

## Isolation and bounds

The harness selects an unused display in `:4000` through `:19999`, excluding the
inherited display. Xvfb disables TCP and uses a newly generated 16-byte
MIT-MAGIC-COOKIE-1 secret in a mode0600 authority file. The artifact and runtime
directories are mode0700. The cookie never enters argv, JSON evidence, or logs.
It is deleted after owned clients and Xvfb stop. A subprocess-only environment
sets the private `DISPLAY`, `XAUTHORITY`, and `XDG_RUNTIME_DIR`, removes inherited
Wayland connection variables, and forces the nested X11 software-rendering path.
The parent environment is checked unchanged.

Each injection opens the explicit private display. It checks that the X lock
belongs to the owned Xvfb session PID and selects exactly one window whose
`_NET_WM_PID` matches the owned compositor and whose title is `FluxWM (nested)`.
Motion must fall within that window. Only left-button and wheel buttons are
accepted. Native Xlib operations run in a helper with a two-second subprocess
deadline. The test never opens the default display or injects into the user's
existing desktop. `_NET_WM_PID` and titles are ownership checks within this
controlled server, not an authentication mechanism for arbitrary hostile X clients.

The runtime test deadline is 30 seconds after package preparation. The compositor
also has an 18-second exit timer. The fixture observes seven seconds after ready
and has a ten-second timeout. The JSONL journal is limited to 1024 events/1MiB;
IPC replies to 1MiB. Downloads have size/time budgets; package verification helpers
have subprocess deadlines. Preparation/network and bounded teardown are outside
the runtime deadline. On exit the harness releases its held button, terminates
and reaps owned clients, stops/reaps Xvfb, and removes its cookie. A test timeout
cannot assert that arbitrary kernel operations have a universal deadline.

## Oracles

Both cases first enter input-A at (64,64), press Linux BTN_LEFT272, and keep it
held. Two fixtures are mapped; their actual IPC geometry determines displacement.

| Case | Change while button held | Required release behavior |
| --- | --- | --- |
| `scene-pan` | IPC pans the canvas by the measured A-to-B displacement; composition counter must advance | A receives release, then B receives enter without further motion |
| `held-motion` | Real pointer motion moves from A to B; the scene receives no pan/action afterward | A receives release, then B receives enter without further motion |

A wheel event during the grab must reach A; B must not receive an enter during
the held interval. After stationary left-button release, B must receive enter
within one second, then receive wheel, motion, and a complete left click. The
fixture must finish with `ok:true`. Its own `ok` confirms instrumentation and
protocol completion; the harness supplies the routing oracle. No window close
operation is used, because the fixture treats unexpected close as failure.

## Observed qualification, 2026-10-17

Both cases were executed against both preserved binaries with the same fixture
SHA256 `48bd92e0c38631e58202a40ca3ca52565063a1efadf2e5a8ebf675ed430f472a`.

| Executable SHA256 | Scene pan | Held motion |
| --- | --- | --- |
| Before: `a47f2df60790425a06f2f64176f27ebbffc41d6c4f02d3cda5a17eea6f9ad239` | FAIL: `M1_missing_focus_after_release` | FAIL: same code |
| Fixed `f637c21`: `ee3e1574d17eb0db9d529228e6f9b897ac61434411d770bb650ce8f3142b6e51` | PASS, normal fixture completion | PASS, normal fixture completion |

Public local evidence directories, respectively:
`/tmp/fluxwm-input-before-evidence2`, `/tmp/fluxwm-input-after-evidence`,
`/tmp/fluxwm-input-motion-before`, `/tmp/fluxwm-input-motion-after`.
The scene-pan before journal records A press514ms, A wheel706ms and A release1080ms,
with no subsequent B enter before the one-second observation deadline. Fixed:
A release439ms, A leave439ms, B enter439ms, B wheel518ms. The held-motion before
journal records A release707ms with no subsequent B enter before the deadline.
Fixed: A release708ms, A leave708ms, B enter708ms, B wheel788ms, B click
release1111ms. These times are fixture-relative observations, not latency bounds.

A final held-motion run of the committed harness also passed at
`/tmp/fluxwm-input-motion-final`. After all five runs, the private Xvfb PID and
X socket were absent and the authority file was deleted.

All four qualification runs authenticated to a private 1280x800 compositor window, preserved
the parent environment, and used package `xorg-server-xvfb-21.1.25-2-x86_64.pkg.tar.zst`
with SHA256 `52c6d0394109b0bdfd0e4d00dd8d89d7e38bfe7576d96145179f216480eb7564`
and the fingerprint above. `package-verification.json` retains public GPG status.
Cached bundle used: `/tmp/fluxwm-xvfb-verified-h8kd45_1`.

To reproduce these invocations, select the desired preserved executable, append
`--bundle /tmp/fluxwm-xvfb-verified-h8kd45_1`, and select the stable fixture with
`--fixture /home/ahmad/Documents/Projects/fluxwm-layout/target/debug/fluxwm-fixture`.
The before executable was `/tmp/fluxwm-input-before/fluxwm`; the fixed executable
was `/home/ahmad/Documents/Projects/fluxwm/target/debug/fluxwm`. Temporary evidence
paths are local run artifacts and must be archived before cleanup.

The public evidence allowlist is `result.json`, `package-verification.json`,
`input.events.jsonl`, `ready.json`, `compositor.stdout`, `compositor.stderr`,
`fixture.stdout`, `fixture.stderr`, and `xvfb.log`. Archive only those files.
Do not archive authority files, cookie bytes, full environment dumps, or private
GPG/cache directories. The tool and executable hashes establish identities;
they do not make temporary paths durable.

This proves these two regressions on the nested X11/software-rendering path.
It does not qualify DRM/libinput devices, native host Wayland input, multiple
buttons/devices, touch, tablet, gestures, pointer constraints, relative pointer,
interactive XDG move/resize, client destruction during a grab, keyboard grabs,
or the full production input matrix. Those remain separate tests.
