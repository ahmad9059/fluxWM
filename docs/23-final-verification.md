# Final documentation verification

Date: 2026-10-17. Scope: expanded engineering plan and research evidence, not a working compositor.

## Checks actually performed

- Inspected upstream source files at exact commits and recorded SHA-256 values. Rechecked all 38 cached file hashes against the source manifest.
- Compared all 13 inspected protocol XML files with installed distro package files: 12 byte-identical, one DMA-BUF clarification difference preserved. Recorded ownership for every path. No signature/ancestry qualification is claimed.
- Built the narrow pinned Rust/mlua probe and reran it locked/offline from cache. Both embedded and standalone Lua 5.4.9 probes caught five ordinary instruction-hook errors; no conclusion about every possible limiter or a production helper follows.
- Parsed/evaluated the Lua example with explicit Lua 5.4, ran the fixture contract validator, and rejected eight malformed fixtures with expected diagnostics.
- Verified exactly ten phase work packages, local Markdown file links, case JSON/Markdown agreement, 94 full cases, 11 variants, declared feature prerequisites and qualification gate assignment.
- Ran three independent Claude review passes, addressed their substantive findings, preserved original reports and scope limitations, and recomputed frozen input hashes after each frozen review completed.

The executable check report is [documentation-checks.json](evidence/documentation-checks.json). It includes passed status **only for its named document/artifact checks**, alongside explicit limits. It does not prove all prose is correct or that runtime behavior exists. The [review summary](reviews/final-summary.md) distinguishes independently reviewed contracts from subsequent low edits and administrative additions.

## Reproduce

```sh
python3 tools/check_docs.py
lua5.4 docs/examples/check-config.lua
cargo run --locked --manifest-path docs/evidence/probes/mlua-hook/Cargo.toml --target-dir /tmp/fluxwm-mlua-probe-target
```

Optional source-cache verification uses `--source-cache PATH`, with the manifest's repository-ID/path layout. Cargo reproduction needs dependency access or a populated cache; the saved offline result was cached, not a clean-machine timing. The Lua fixture validator is not a hardened production config parser.

## Outstanding implementation evidence

No FluxWM compositor executable, compositor Cargo workspace, protocol handler implementation, renderer qualification or performance benchmark exists. Required platform/library selections and staged runtime gates remain in [B01–B21](13-production-audit.md). The plan is ready to start phase 1; production readiness requires the actual phase artifacts and supported-hardware/session evidence.
