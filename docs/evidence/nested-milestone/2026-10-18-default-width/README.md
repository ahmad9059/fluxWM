# Configured future-column width evidence — 2026-10-18

This bundle qualifies fixed/fraction/default-preset policies for new window admissions while preserving existing widths. Read the [contract](../../../development/config-widths.md) and [independent review disposition](../../../reviews/default-width-2026-10-18/disposition.md). Full rules, preset cycling, device/output transactions, native sessions and production readiness remain open.

## Source and checks

[execution.json](execution.json) records final source `87d52d0`, implementation `74c97bd`, reviewed source `ccfdb61`, exact binary/helper/fixture/harness/lock/source hashes and command results. `final-source/` retains selected source bytes. All **162 named Rust tests**, workspace build, Clippy with warnings denied, rustdoc with warnings denied and formatting pass. Raw checks/enumeration are in `checks/`.

The final private width case passes with six real SHM-backed windows. Desired/effective state is cross-checked against stable live IDs, actual model geometry, initial frame callbacks and every nonzero configure width received by each client (with expected heights). Initial zero-size configure values are allowed. The qualified viewport is 1280 × 800; admissions have widths **400, 760, 320, 600, 600, 480**:

1. One-based preset 1 resolves to fixed 400.
2. Fixed 760 applies to the next member; the first keeps 400.
3. Fraction 0.25 gives 320; earlier fixed widths remain.
4. Editing preset 1's definition to 600 affects the next admission.
5. Invalid preset 99 changes neither the applied generation nor desired/effective state; a subsequent admission still gets 600.
6. Recovery to fixed 480 applies to the next member, retaining previous identities/widths.

`widths/` retains Lua attempts, IPC traces/snapshots, fixture events/configure sizes, commands/results and cleanup. The invalid preset is rejected by the schema; it does not reach a native width-conversion error. Model/resolver tests separately cover defensive errors, both modes, viewport changes, min/max constraints, full failure preservation and retry/remap without focus theft.

The same final executable/fixture also passes **six existing configuration cases**, **six default private regression suites** and **three host-focus cases** in `configuration/`, `regressions/` and `focus/`. The response oracles now include requested width tags/preset definitions, normalized effective policy and updated unsupported-setting categories. Non-configuration suites explicitly use `--no-config`.

All displays are owned authenticated private Xvfb instances; no host-session input is injected. Each run revalidates its signed bundle and records owned authority/server/socket removal, runtime endpoints removed before directory deletion, unchanged parent environment and unchanged executables. Package/signature metadata is public; executable files, screenshot pixels, cookies, authority files and full environments are excluded from this bundle. [public-origins.json](public-origins.json) records copied source paths and raw byte hashes.

## Deliberately failing proofs

`admission-mutation/` retains the `74c97bd` source baseline and a mutation which restores the old hardcoded half-viewport policy only at actual column creation. Effective metadata still reports the requested 400 policy and extent preflight still uses it, but the new member's real width becomes 640. The actual private harness exits 1 specifically with `wrong configured width for member0`. This establishes a geometry oracle beyond metadata-only assertions. Its failure records are intentional, not a qualified production run. The exact original source hash is restored.

`preset-mutation/` retains a separate mutation from the reviewed-code checkpoint: using the preset index directly instead of subtracting one. The valid default preset resolves to the wrong policy and the unit assertion fails with exit 101. Source is restored and repaired component tests pass. Original and mutated source/commands/logs are retained; these fault experiments predate the later low test/comment corrections and are not relabeled as final-binary mutation runs.

## Review and limits

Claude inspected frozen `ccfdb61` and found no reachable high/medium defect within its read scope. All 72 input hashes match and the actual trace has 18 calls. It executed no tests. Later author-only fixes correct the fixture comment, clarify constraint-adjusted Canvas seeding, use Rust-compatible positive rounding and strengthen generation/all-wire-size assertions. The extra retry/remap model test also follows review. Those changes are identified in the disposition rather than presented as independently reread.

Actual runtime resize with fractional columns, remap/rejected-buffer retry under changing defaults, and client-limit/default combinations remain unqualified at the whole-compositor boundary despite model/source coverage. Initial frames and configures do not establish physical presentation. No arbitrary scheduling, hostile-client fairness, universal RSS/latency, native or soak guarantee is inferred from these local results. The bundle's manifest seals final bytes, excluding itself; raw console whitespace is preserved.

[Whitespace verification](diff-check.json) records the exact raw-console exclusions from the otherwise passing staged source/documentation check.
