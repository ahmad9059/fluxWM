# Configuration production audit

Audit date: 2026-10-17. Source reviewed: configuration commits
`d8d8722891f8c42261d03798ace8181e5828d65f` and
`7987e8833fa908285ae15e061ab79d99a473bf65`. This is an implementation-area audit,
not the requested independent Claude review. It does not certify phase 6 or a usable
production session. Findings below distinguish code evidence from remaining integration
and qualification work.

## Evidence examined and reproducible checks

The current code is [owned schema](../../crates/flux-config/src/schema.rs),
[parent structural preflight](../../crates/flux-config/src/bounds.rs),
[process supervisor](../../crates/flux-config/src/supervisor.rs),
[Lua helper](../../crates/fluxwm-config/src/main.rs), and
[containment integration tests](../../crates/fluxwm-config/tests/containment.rs).
Compared contracts: [Lua](../06-lua.md), [resource/security](../17-resource-security.md),
[review resolutions](../21-review-resolutions.md), [phase 6](../phases/06-lua-ipc.md),
and [the configuration fixture](../examples/config.lua).

Commands run in the isolated configuration worktree:

```sh
cargo test -p flux-config -p fluxwm-config
cargo clippy -p flux-config -p fluxwm-config --all-targets -- -D warnings
cargo run -p fluxwm-config -- --check docs/examples/config.lua
```

Observed results: 8 library tests, 3 helper unit tests, 7 integration tests passed;
clippy passed with warnings denied. The real documentation fixture produced 4,902
bytes of pretty normalized JSON. No device/display connection or configured command
execution was needed. These results cover this component on this development Linux
host and debug profile. They do not establish release-profile behavior, supported
kernel coverage, maximum RSS, render latency or a hardware/session lifecycle.

## Contract coverage and precise limits

| Requirement | Current evidence | What the evidence does not prove |
| --- | --- | --- |
| Lua 5.4 selection | Exact mlua Git revision and features in helper manifest; `pinned_build_really_embeds_lua54` checks VM `_VERSION` before globals are restricted | Future lockfile changes or every distribution build |
| No Lua in parent library | flux-config manifest has serde/regex/libc/core dependencies and no mlua; VM creation exists in the separate executable | The final compositor dependency graph must still be inspected after integration |
| Restricted libraries | Integration fixture explicitly asserts forbidden global names are nil, RNG/dump absent; source builds only math/string/table plus selected pure base functions | An OS syscall sandbox or protection against native-code VM compromise |
| Plain returned data | Hostile fixture rejects cycles/depth, functions, sparse/mixed/invalid-key tables and nonfinite values; metatable unit test sets a hostile `__pairs` and observes rejection without callback execution | Full fuzz coverage of all Lua/Rust conversion states |
| Parent structural checks | Streaming preflight rejects depth above 32, more than 100,000 values and trailing JSON before typed allocation | An exact whole-process memory bound |
| Strict typed schema | Unit tests reject competing width/key/output tags, unknown nested fields, unknown fields on unit actions, bad references, NUL argv, invalid ranges and duplicate normalized chords | Backend acceptance, font/GL/input/output capability or executable availability |
| Regex dialect and bounds | Rust RegexBuilder with nesting 64; per-pattern ceilings; aggregate engine budgets; unsupported lookaround fixture rejected | Acceptable total metadata-evaluation time, actual RSS or compile cancellation latency |
| Deny-first privacy | A matching deny beats a later allow; nonprivacy fields still merge later-wins; existing sticky deny survives changed metadata in tests | Stored privacy history, capture admission, unknown relationships or lock behavior in the compositor |
| Ordered output merging | Test verifies a later wildcard replaces scale and preserves an earlier adaptive_sync field | EDID ambiguity diagnostics, real connector churn, KMS transactions or rollback |
| Wall deadline | Mock helper sleeps past 150 ms and returns Timeout; RAII guard unit proves owned child is killed/reaped via ECHILD | Exact one-second maximum elapsed time under arbitrary OS scheduling stalls |
| Hostile Lua termination | Infinite loop, nested pcall loop, allocator growth and huge native string work all fail within a three-second test envelope; a new VM remains usable | That every hostile Lua case specifically reached the parent deadline; some fail earlier through hook/allocation errors |
| Pipe bounds | Mock helper overflow tests require the specific result/diagnostic overflow errors; valid JSON plus junk requires a trailing-characters error | Kernel fault injection, short-write exhaustive schedules or shutdown cancellation |
| fd/environment containment | Test deliberately clears CLOEXEC on an open descriptor and proves it is absent after exec; verifies environment, no_new_privs, AS/CPU/NOFILE/CORE limits | A filesystem namespace, seccomp, every possible inherited-fd arrangement or kernel fallback |

Two concrete defects were found and corrected during implementation/audit:

1. Serde's internally tagged unit action variants accepted extra fields despite the
   enum's deny_unknown_fields declaration. The configuration Binding decoder now
   checks an exact action-field allowlist before decoding core Action. A regression
   rejects `{name="lock",argv={}}`. Other users deserializing core Action directly
   must not assume this configuration-specific defense protects their boundary.
2. A 1 MiB per-pattern engine ceiling could aggregate across 2,048 patterns. The
   compiler now apportions a 32 MiB engine-size budget over all patterns, with the
   same apportioning for lazy DFA caches. A regression accepts one rule containing
   `a{5000}` patterns, then rejects 1,024 such two-pattern rules by engine-size limit.
   This is an accounting control, not a measured 64 MiB RSS guarantee.

## Prioritized unresolved production blockers

Priorities refer to completing the intended product. These are not claims that an
unfinished compositor currently violates a deployed security guarantee.

| ID | Priority | Missing contract/evidence | Required closure |
| --- | --- | --- | --- |
| CFG-01 | P0 | No config generation/transaction owner in these crates; load_with_helper is a blocking API and can be called concurrently | Compositor worker integration with exactly one active helper, one coalesced latest request, stale-generation rejection, shutdown cancellation and atomic between-dispatch publication; test reload storms and out-of-order completion |
| CFG-02 | P0 | A typed valid snapshot does not apply outputs/devices or preserve old hardware state | Implement desired/effective state, capability diagnostics, transactional output/device application, rollback and last-known-good state; inject every apply failure boundary |
| CFG-03 | P0 | Privacy evaluator accepts an already classified is_dialog boolean; stored sticky deny and relationship classification are caller-owned | Represent unknown/unclassified relationship state conservatively; propagate parent/privacy changes; run privacy admission before each capture job; test metadata churn, parent destruction, lock epochs and trusted explicit clearance |
| CFG-04 | P0 | Rule dynamic flag is retained but evaluate_rules merges all placement fields on each call | Separate initial placement, dynamic placement and always-current privacy evaluation in the integration API; ensure title-only privacy reevaluation cannot unexpectedly move a static-rule window |
| CFG-05 | P1 | Key normalization validates syntax/modifiers, not xkb symbol existence or current keymap semantics | Resolve canonical keysyms with xkbcommon, test Shift/base symbols, layout switches, evdev offset, duplicate conflicts after real keymap resolution and keymap compile failure |
| CFG-06 | P1 | Only the current core action subset is decoded, whereas the target document lists more families | Add typed workspace/output transfer, maximize/minimize/restore, screenshot/search and other promised actions alongside real command handling and input/IPC shared validation; publish supported-action reference |
| CFG-07 | P1 | Output schema currently includes match/scale/adaptive_sync only | Add explicit desired mode/refresh/position/transform/enable policies with qualified backend transactions; preserve disconnected rules and report ambiguous physical matches |
| CFG-08 | P1 | Starting CPU/AS/allocator/result ceilings have no workload qualification | Measure debug/release helper startup, peak RSS, native conversion/serialization overhead, large valid configs and maximum-rule compilation/evaluation; tune per supported architecture/kernel; do not confuse AS with RSS |
| CFG-09 | P1 | close_range failure fails closed; no compatibility matrix or fallback is qualified | Establish minimum kernel/distribution support, test ENOSYS/EPERM/rlimit failures, and provide an explicitly reviewed equivalent fd-closing path only if older support is required |
| CFG-10 | P1 | No built-in async cancellation API; deadline covers child exchange but not subsequent parent regex compilation | Implement worker ownership/cancellation and a bounded compile scheduler; measure worst-case accepted compilation and release work, ensure dispatch never waits on it |
| CFG-11 | P1 | Autostart/spawn are validated data only | Supervise once-per-session autostart and restart_on_change ownership, argv execution without shell splitting, session teardown and process failures; test successful config-check has no side effects |
| CFG-12 | P1 | Default terminal binding names foot but installation, startup UI and fallback session are outside component | Packaging dependency and usable recovery path; invalid config must preserve working inputs/outputs and expose actionable accessible diagnostics |
| CFG-13 | P1 | No fuzz/sanitizer/soak corpus or allocation/syscall failure injection | Fuzz source and helper-result boundaries, execute repeated reload exhaustion/timeout soak, test EMFILE/spawn failures/EPIPE/short reads, malformed helper results and interrupted supervision |
| CFG-14 | P2 | Lua chunk name is always config.lua; several range errors identify a section rather than an exact list index | Carry bounded source/display path provenance and improve field-level paths without exposing environment or secrets; test UTF-8 diagnostic truncation and traceback limits |
| CFG-15 | P2 | Table schema/defaults and documentation are maintained separately | Generate supported reference/default output from one schema contract and validate fixtures in CI; remove stale no-parser claims once integrated |

## Documentation contradictions and interpretations

`docs/06-lua.md` and `docs/examples/config.lua` still say there is no parser. That was
true of the earlier planning artifacts; it is no longer true after the commits above.
The implementation-status document [config.md](config.md) explains the new boundary.
The overall documentation index should point users to the implementation status and
retain a clear distinction between intended phase contracts and currently supported
runtime behavior.

The target Lua document lists broad action families and output mode/transform support.
The current executable accepts only the smaller explicit core/schema subset documented
above. This is unfinished implementation, not evidence that the original requirement
has been met. Likewise `--check` establishes schema acceptance only; it cannot claim
hardware validation or prove that a desired setting was applied.

The resource document allows up to 4 MiB across modules if modules are supported.
No module loader is exposed in the implemented baseline: require/package/load/file
helpers are absent. This avoids an unqualified path/symlink policy. Add module support
only with directory-handle snapshot/path/cycle/aggregate-source tests, or continue to
state it is unavailable. Coroutines and runtime callbacks remain unavailable by design.

Lua's empty table is structurally ambiguous. Conversion treats empty tables as arrays
only for declared array-field names and otherwise as records; unknown fields still fail
the schema. Lua integer subtype is retained when converting to JSON, so OS integer
fields should use integer literals/values. An integral Lua float such as `code=35.0`
is currently rejected by typed integer deserialization. If users should be allowed that
form, normalize mathematically integral values explicitly and add range/precision tests;
do not silently weaken exact integer guarantees.

The Linux hardening path marks every fd above stderr close-on-exec before exec; it does
not close the Command exec-error descriptor prematurely. no_new_privs and rlimits
are real controls tested here; there is no seccomp, mount namespace, cgroup or absolute
native compromise containment claim. The API relies on a trusted helper executable.
Tests use `/usr/bin/python3` only for fault-injection helper programs; runtime evaluation
does not depend on Python.

## Required review focus

The independent reviewer should inspect, rather than infer from green tests: unsafe
pre_exec async-signal-safety, exec-error-fd behavior, close_range portability, child
identity/reaping, nonblocking transport deadline fairness, raw Lua traversal, serde
unknown-field edge cases, regex aggregate limits, metadata privacy persistence, and
which contracts require integration rather than local helpers. A clean local test run
must not close CFG-01 through CFG-15 without their specific evidence.
