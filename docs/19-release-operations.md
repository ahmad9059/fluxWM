# Production qualification, release, and incident operations

This is a required operational specification for implementation phases, not a declaration of current readiness. Read [blockers](13-production-audit.md) and [case oracles](15-edge-cases.md). None of the future FluxWM runtime qualification has been executed.

## Support matrix contract

Before a release candidate, declare tested architecture, distribution/version, kernel, Mesa/proprietary driver, GPU model, renderer/backend, seat mechanism, output modes/scales/refresh, capture backend, Xwayland route, IME, accessibility stack, and known failures. Split supported, experimental, unsupported, and untested explicitly. A vendor name alone is not a support matrix.

One passing Intel machine does not qualify all Intel devices, and one passing NVIDIA configuration does not qualify every driver branch. Hybrid tests need both application render-device choice and physical output-device placement. Mark physical input, VRR, suspend, color, hardware cursor, direct scanout, and external docks as separate test dimensions.

The initial complete desktop target includes single-seat SDR/GLES. If no GPU or atomic KMS path is usable, use a tested fallback or fail with a clear diagnosis and return device/session control. Do not silently claim a software/nested fallback provides equivalent native production behavior. Outputless continuation needs its own policy and accessibility test.

## Reproducible build and supply chain

Pin toolchain, application lockfile, Git dependencies, generated protocol input revisions, shader generation tools, and required native ABI/library versions. Record provenance for downloaded/generated artifacts. Include dependency licenses/notices and an SBOM or equivalent machine-readable inventory. Separate source license review from security advisory scanning; neither alone proves safe redistribution or absence of vulnerabilities.

Run dependency advisory/license checks in CI with recorded database revision and evaluate findings. Builds should not fetch unpinned scripts or execute arbitrary new network steps during package installation. Generate release artifacts in a controlled environment, record hashes and build metadata, and verify a clean rebuild where feasible. Document reproducibility exceptions such as timestamps or driver-generated shader caches.

Build profiles must be measured: LTO/codegen units/panic strategy/allocator choices affect size, startup, link time, crash behavior, and profiling. Do not prescribe universal `target-cpu=native` for distributed binaries. Keep debug symbols/build IDs available for diagnostics. Any `panic=abort` choice must match the stated crash/recovery limitations.

## Session units and helper supervision

Define the lifecycle owner of compositor, panel, IME, locker, authentication agent, portal backend, idle service, and X11 bridge. Essential service readiness has a handshake and timeout. Optional helper failure is surfaced but must not destroy unrelated work. Restart only supervised children with bounded backoff and a maximum failure rate; do not launch an infinite crash loop.

Environment updates happen only from a full session launcher. Nested tests own their display/IPC sockets and child process environment and do not activate host graphical-session targets. Logout tears down session-owned services without killing unrelated SSH/tmux/user services. External application launch uses appropriate session/application ownership and explicit argv; fd inheritance is audited.

## Failure and recovery decision table

| Failure | Immediate behavior | Evidence / limitation |
| --- | --- | --- |
| Config helper timeout | Kill/reap helper, keep last good generation | CFG-01/05; no orphan process accumulation |
| Portal backend dies | Stop affected capture sessions and report | CAP-01/02; cannot erase already delivered frames |
| Satellite/Xwayland dies | Report X11 app connection loss, clean display/helper state | CAP-07; no transparent app restoration claim |
| Locker dies | Keep safe coverage and locked authority, launch only trusted replacement policy | SEC-02; recovery cannot bypass authentication |
| GPU/device lost | Stop using old handles, preserve security, recover only if proven safe | GPU-08/OUT cases; clean session failure may be required |
| Compositor crashes | Session manager reclaims control and ends/restarts session per explicit policy | Native app connections are lost; persistent layout is only a placement hint |
| Native startup fails | Return seat/device control; leave actionable log | No retry loop that strands VT |
| IPC flood/slow reader | Bound/reject/disconnect client | CFG-06/07; lock and input still progress |
| OOM/fd exhaustion | Reject optional work where possible; follow realistic fatal policy when allocation cannot recover | OPS-02; cannot promise universal recovery |

A compositor crash while locked cannot be solved solely by an in-memory lock boolean in the dead process. The supported display-manager/session-launch path must be tested for secure return to a greeter or explicit logout; never automatically restart into an unlocked preserved session and call that lock recovery. Physical access/VT policies and other sessions remain outside guarantees unless explicitly integrated.

## Release evidence bundle

Each release candidate includes build ID/source revisions; package hashes; protocol advertised-version inventory; hardware/application/companion versions; case pass/fail/blocked results; raw timing/memory/fd/GPU records; screen-reader and keyboard-only journey; clean install/upgrade/uninstall record; known issues; and security-sensitive changes since last release.

Use a machine-readable result record with case ID, status, build, environment ID, command/procedure, observation, artifact hashes/paths, timestamp, and reviewer. Keep secrets/pixels/titles redacted by default. A test report naming an artifact that cannot be found is incomplete evidence. Release-blocking changes invalidate affected old results even if a previous build passed.

## Soak and performance qualification

Run the declared 24-hour churn workload, plus interactive real-app sessions, with a stable baseline and resource telemetry. Track warmed caches separately from monotonically growing live allocations. Include repeated capture start/stop, lock/unlock, output changes, config helper failures, and X11/IME use. Avoid uncontrolled resource exhaustion/reset experiments on a personal desktop.

Repeat benchmark samples after thermal stabilization and record background load/power state. Publish distribution and confidence/variance instead of only best results. Set approved regression thresholds after measured baselines; provisional docs budgets do not become validated merely because the compositor subjectively looks smooth. Battery power requires platform power instrumentation; idle CPU alone is not battery life.

## Rollout and maintenance

First release to an explicit tester cohort with a rollback package and known recovery route. Freeze schema/IPC compatibility policy and document migration/deprecation. Retain a stable supported dependency set and review upgrades in isolated branches. Security fixes for buffer lifetimes/lock/capture get targeted regression cases plus the broader sensitive fast-path review.

Provide a minimal redacted bug report command collecting build/features, protocol capabilities, output/device identity hints, session/companion versions, and recent rate-limited errors. User approval is required for sharing optional sensitive diagnostic contents in the product workflow. Triage reports by reproducible case, crash/security impact, hardware scope, and workaround. Production maintenance needs named maintainers and a vulnerability contact; this plan cannot assign people or promise support capacity that does not exist.
