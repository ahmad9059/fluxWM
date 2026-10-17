# Runtime configuration smoke test

`tools/smoke_config.py` checks asynchronous Lua configuration publication against
real nested compositor processes and their Unix IPC endpoints. It creates its own
authenticated private Xvfb through the shared `smoke_input` helpers. It does not
inject input or change the caller's display environment. Package verification,
private display authentication, and cookie handling follow
[private-input.md](private-input.md).

Build `fluxwm` and its sibling `fluxwm-config` helper, then run:

```sh
python3 tools/smoke_config.py --binary target/debug/fluxwm
```

Use `--bundle PATH` for an existing official signed Xvfb package/signature and
matching `extra.db`; verification repeats on every run. `--artifacts PATH` chooses
a new evidence directory. The default is `target/smoke-config/<unique-id>`.
The helper must be named `fluxwm-config` beside the selected compositor binary.
The harness records hashes before testing and verifies both executables remain
unchanged. The runtime itself resolves the helper path before worker startup.

Three independent compositor instances exercise:

1. `--no-config`: configuration remains disabled with null generations and default
   binding source; capabilities disable reload and `config.reload` returns
   `not_ready`.
2. An explicit startup file with an unknown binding symbol: startup reaches
   readiness, configuration fails with a diagnostic error, applied generation and
   desired config remain null, default layout stays gap8/strip, and binding source
   remains `development-defaults`.
3. A valid startup file with gap17/canvas and three bindings (quit/reload/close):
   publication must complete. A valid reload changes gap29/strip. Invalid Lua and
   then an unknown binding symbol must each produce a new failed requested
   generation while preserving the previous applied generation, effective
   settings, workspace state, and binding diagnostics. A final valid reload
   recovers with gap11/canvas and clears the error.

Each accepted reload must return a strictly increasing canonical decimal-string
generation. The test waits for the matching requested generation and terminal
status rather than treating acceptance as publication. A successful snapshot
cross-checks diagnostics, desired/effective layout, binding counts, fixed startup
seat keymap identification, unsupported setting categories, and the live workspace
mode. Rejected configurations include a private `touch` autostart marker; that
file must remain absent throughout observation and after teardown. This checks
that particular forbidden side effect, not arbitrary external effects.

The unknown symbol is deliberately valid schema data:
`Super+FluxwmDefinitelyUnknownKeysym`. The standalone Lua helper accepts its data
shape; rejection must happen during runtime binding compilation against the
actual startup keymap. Schema-only rejection would not exercise that boundary.

Each compositor case has an18-second absolute monotonic deadline and a17-second
runtime exit timer. IPC reads have length bounds and honor the case deadline;
trace data is capped at1200 records/16MiB and logs at4MiB each. On success and
failure, owned processes are terminated/reaped, sockets checked removed, the
private display stopped, and its authority file deleted. No configuration file is
written to the user's home or configuration directory. The temporary config is
replaced atomically between requests, and every attempted source is retained.

Evidence includes `summary.json`, public package verification, per-case
`command.json`, `result.json`, `ipc-trace.json`, Lua attempts, snapshots, and bounded
stdout/stderr logs. Never archive authority files or cookie bytes, full environment
dumps, or the whole private package/GPG directory. Exit zero means every runtime
assertion and cleanup check passed; exit one retains evidence for diagnosis.

## Verification status

The initial harness passed Python syntax compilation. Its generated valid Lua
passed the real standalone helper; the unknown symbol also passed helper schema
validation, and invalid Lua returned a syntax error. Runtime assertions require
the compositor integration; these static/helper checks alone do not establish a
runtime pass. Record actual run paths and executable hashes with any runtime
qualification claim.

This suite checks configuration publication and retention, not physical key
activation, publication while a key/button/grab is held, helper crash/hang limits,
file replacement races, superseded concurrent generations, arbitrary unsupported
setting behavior, or sustained client frame progress. Responding to IPC is not
proof that clients continue rendering. Those need separate evidence.
