# Narrow mlua hook evidence probe

This is a standalone research experiment, **not FluxWM compositor code**, a selected production dependency set, or the future config supervisor. It pins the inspected mlua Git commit and Lua 5.4/vendored features, with a committed lockfile.

Run from the repository root, with build output outside the documentation tree:

```sh
cargo run --locked --manifest-path docs/evidence/probes/mlua-hook/Cargo.toml --target-dir /tmp/fluxwm-mlua-probe-target
```

The finite outer loop catches five ordinary instruction-hook errors through Lua `pcall`. It proves those errors alone do not force outer evaluation to terminate in this specific embedding. It does not prove every possible execution limit can be bypassed, establish hostile-code containment, validate a config schema, or test process termination. The production design requires a separate supervised process with a parent deadline.

See [recorded stdout](../../mlua-probe-output.txt) and [execution metadata](../../mlua-probe-result.json). Reproduction requires Rust/C tooling and dependency access or an existing Cargo cache. Vendored build inputs are governed by the pinned crate and lockfile, not the system's default `lua` executable.

The recorded result uses `--locked --offline` and existing Cargo/build caches after an initial successful build (18.62 seconds observed). For the exact cached rerun, add `--offline` to the shown command. This is not a clean-machine timing. The pinned `lua-src 551.0.2` Lua54 mapping and bundled header identify patch level **5.4.9**; `_VERSION` alone identifies only the 5.4 family.
