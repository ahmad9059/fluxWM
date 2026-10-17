# Phase 6: Lua configuration, rules, IPC, and CLI

Status: planned. **Dependencies:** Phases 4–5.

## Objective

Deliver the capability in this phase while retaining every previously established product invariant. See the [roadmap](../10-roadmap.md) for sequencing and contract-based scheduling requirements.

## Implementation tasks

1. Implement schema v1 and typed defaults from the configuration document. Pin Lua 5.4/mlua features and reject unknown fields and nonfinite values.
2. Build a restricted supervised config helper process with fresh VM, memory/instruction limits, bounded module loading if supported, and no render-thread evaluation.
3. Compile bindings/actions, patterns, output/device rules, and animation references into immutable Rust snapshots. Implement deterministic precedence.
4. Add debounced reload with generations, stale-result rejection, logical atomic swap, and hardware transaction rollback/effective-state reporting.
5. Implement autostart once-per-session supervision and argv-based spawn. Preserve working defaults on startup config failure.
6. Add Unix socket authentication, versioned length-prefixed IPC, bounded clients/subscriptions, typed errors, event sequences, and stale-ID checks.
7. Implement fluxctl query/action/config-check/reload/diagnostics commands and generate user reference/help from shared schemas where feasible.

## Required artifacts

Working Lua config loader and example, schema reference, configuration validation CLI, reload diagnostics, rule engine, supervised autostart, IPC specification/client, parser and failure tests.

## Verification procedure

Validate and load the supplied example. Change gaps/curves/bindings live and inspect generation. Try infinite loop, coroutine library absent, memory blowup, missing module, native library load, duplicate chord, invalid output mode, stale helper result, and partial IPC frames. Slow subscribers cannot delay input or lock. Reload cannot duplicate companion processes.

## Exit gate

Users can configure all implemented features through documented Lua data, failed reloads preserve a usable session, and IPC operations share core command validation. No Lua runs during rendering; runtime configuration and checked example agree.

## Engineering notes

Use Lua and desktop/IPC contracts. Keep proposed commands clearly marked until implemented. Phase 8 extends the same schema for canvas and accessibility rather than introducing a second configuration language.

## Review record to fill during implementation

Record build revision, dependencies, environment, commands/scenario IDs, raw result locations, failures, remaining limitations, reviewer, and date. Mark each task complete only when its artifact and relevant verification exist. A phase summary without evidence does not pass the gate.

## Production audit supplement

Use the [production blocker register](../13-production-audit.md) and [review decisions](../21-review-resolutions.md). The exact case exit-gate list for this phase is in [phase-gates.json](../evidence/phase-gates.json); full cases and early variants are distinct. Do not require features scheduled for later phases. `introduced_phase` is design/harness work only, not a full-case pass.

Retain [ordering contracts](../14-state-machines.md), [resources/trust](../17-resource-security.md), [desktop integration](../16-integration-contracts.md), [typed implementation contracts](../20-implementation-contracts.md) and the [window-state matrix](../22-window-state-matrix.md). Substage closure evidence for shared blockers is listed in the blocker register. Numeric limits require qualification.
