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

Six independent compositor instances exercise (use `--case baseline`, `--case input-publication` or `--case path-retention` or `--case geometry-rollback` for focused runs):

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

4. A real input fixture maps two windows. Holding Alt defers a new candidate after
   diagnostics prove that exact generation is prepared. Repeated observations keep
   the old applied generation/settings for at least 350 ms. Releasing Alt permits
   publication. Before release, another reload supersedes that already prepared
   generation; the newer candidate is observed prepared and only its generation
   applies. The initial table has three entries and no Alt+a: A must reach the
   client and mode remain strip while the fourth-entry replacement is only
   prepared. After publication, configured `Alt+a` (absent from development
   defaults and the initial user table) changes the live workspace to canvas. Its A press/release is consumed, while a
   later plain A reaches the client; complete A/Alt counts remain balanced.
5. Removing the selected source and then requesting schema-valid gap2048 each
   produces failure while preserving the old generation, settings and binding
   table. Gap1024 is accepted; gap1025 fails with the exact nested-limit message
   and retains gap1024. A valid gap19/canvas reload recovers.
6. A dedicated protocol fixture maps a SHM-backed window with minimum width
   999,999,984 and initial gap13. Reloading gap32/canvas with an extra binding must
   reach the exact live-geometry error (the gap is below the model cap), preserve
   ID/geometry/focus/workspace, mode, binding count and applied generation, and then
   recover with gap8. The fixture observes a real initial frame and remains live
   through the checks.

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

Each compositor case has an 18-second absolute monotonic deadline and a 17-second
runtime exit timer. IPC reads have length bounds and honor the case deadline;
trace data is capped at 1200 records/16 MiB and logs at 4 MiB each. On success and
failure, owned processes are terminated/reaped, sockets checked removed, the
private display stopped, and its authority file deleted. No configuration file is
written to the user's home or configuration directory. The temporary config is
replaced atomically between requests, and every attempted source is retained.

Evidence includes `summary.json`, public package verification, per-case
`command.json`, `result.json`, `ipc-trace.json`, Lua attempts, snapshots, and bounded
stdout/stderr logs. Never archive authority files or cookie bytes, full environment
dumps, or the whole private package/GPG directory. Exit zero means every runtime
assertion and cleanup check passed; exit one retains evidence for diagnosis.

## Width-aware response follow-up

The [default-width integration](config-widths.md) extends desired/effective layout
summaries with the requested default tag, preset definitions and normalized width
policy. The current strict response assertions include those fields and remove
them from unsupported categories. The [later width evidence](../evidence/nested-milestone/2026-10-18-default-width/README.md)
re-runs all six cases against this response shape, alongside 162 named Rust tests.
The source `8152a96` results below remain the earlier response checkpoint.

## Verification status

The final private run on source `8152a96` passes all six cases. The [retained
bundle](../evidence/nested-milestone/2026-10-18-runtime-config/README.md) records
binary/helper/fixture/harness hashes, snapshots, raw key journal, generations and
cleanup. The same executable passes all six default private suites and three focus
cases. Separate author checks pass 157 named Rust tests, build, Clippy, rustdoc and
formatting. Deterministic controller tests separately check stale/closed worker
publication, a newer request hiding a prepared completion before its worker runs,
and final-dispatch termination sealing. Four separate mutations removing those
checks fail the intended assertions; restored tests pass. A later mutation retaining
only the old table on reload fails the corrected three-to-four-entry real-input
assertion; the original source/executable hash is restored. A separate source-open mutation proves the
FIFO test detects blocking opens. These are named local proofs, not certification.

The real held-input check covers Alt only; it does not qualify every key/button/grab
combination. This suite does not establish physical keyboard behavior, coherent
keymap/device replacement, arbitrary concurrent runtime-generation schedules beyond the prepared-held-input case, helper
crash/hang behavior at the whole compositor boundary, in-place file races, arbitrary
unsupported setting behavior, or sustained client frame progress. Responding to IPC
is not proof that clients continue rendering. Those need separate evidence.
