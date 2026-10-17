# Configured widths for newly admitted windows

The nested runtime now applies `layout.default_width` and resolves its references to `layout.width_presets` when preparing a configuration. The selected policy is published with gap, mode and bindings. It applies to newly admitted columns, including a remapped window or a previously rejected buffer admitted later. It does not resize existing columns or erase a user's manual width changes. Reload acceptance alone does not change the policy; admissions use the last published generation until the new one is applied.

```lua
layout = {
  gaps = 12,
  mode = "strip",
  width_presets = { { logical_px = 400 }, { fraction = 0.25 } },
  default_width = { preset = 1 },
}
```

This fragment belongs inside the returned configuration table. Presets are one-based and cannot reference other presets. If replacing the list with only one definition, select a valid default explicitly: the schema's default still references preset 2. A default may also be `{ logical_px = 760 }` or `{ fraction = 0.25 }` directly. Fractions use the current usable logical viewport width. The existing schema admits pixel widths 1–65536, fractions 0.01–16 and 1–128 preset definitions. Numeric admission is not a memory/performance qualification of every resulting client size.

Each admission copies the policy into its new column. Fixed policies remain fixed as the viewport changes; fractional policies resolve against later viewport sizes. An admission while already in Canvas seeds the new group's size from the constraint-adjusted client strip rectangle, rather than the raw policy. First mode conversion and existing saved canvas geometry follow the established mode/resize rules. Client minimum/maximum constraints still apply to the client rectangle. A constrained client can be narrower than its column container. A rejected future admission leaves current membership, focus, geometry and identity allocation unchanged.

Changing the default affects later admissions, rather than retroactively changing columns created with an earlier default. Changing a preset's definition re-resolves the selected default at the next successful publication; existing columns retain their copied policies. Preset cycling actions are not implemented by this change.

`config.effective.desired.layout` retains the requested default tag and preset list. `effective.layout.default_width` reports the resolved policy as `logical_px` or `fraction`, not one universal pixel width for all current windows. A fraction's actual size depends on viewport and client constraints. Defaults remain half-viewport when configuration is disabled or the default preset list is used. Invalid schema/preset or live-layout application failures preserve the old desired/effective configuration and future-admission policy.

The pure layout model validates that the default policy resolves for its viewport. Reconfiguration of default/gap/mode is failure-atomic; viewport changes that make the default policy unresolvable are rejected. Existing geometry is validated separately, so a valid future policy is not a promise that every later client can fit within aggregate coordinate limits. Key/button/grab deferral and source-generation guards remain the [runtime configuration contract](runtime-config.md).

## Actual verification

The [private width harness](../../tools/smoke_width.py) drives six real SHM-backed windows through fixed 400, fixed 760, quarter-viewport, changed preset 600, retained 600 after invalid preset99, and recovery to fixed 480. It checks stable prior IDs/widths, effective desired/reference state, failure retention, each initial frame all nonzero wire configure widths and expected heights (initial zero-size configures are allowed). It uses an owned authenticated Xvfb, explicit temporary Lua sources and no host-session input.

```sh
cargo build --workspace --locked
python3 tools/smoke_width.py --bundle target/configure-verified-xvfb --artifacts target/new-width-check
```

The artifact path must be new. The existing bundle is reverified; it is not installed system-wide. Records include source attempts, IPC traces, fixture events/configure sizes, executable/harness hashes and cleanup. Pure model tests additionally check both modes, fixed/fraction viewport behavior, individual client limits, invalid defaults and complete failure rollback. Runtime resolver tests cover one-based lookup, out-of-range and recursive references.

The first private run passes on this machine with widths 400/760/320/600/600/480. This does not establish native behavior, physical presentation, arbitrary window-rule precedence, preset cycling, all client constraint combinations, latency or soak qualification. Remaining outer gap/focus/canvas-policy settings, input/output transactions, rules and autostart still need implementation.

Remap/rejected-buffer retry, viewport changes and client size constraints are source/model contracts with component coverage; this width harness does not qualify those combinations at the whole-compositor boundary. Its invalid-preset case reaches schema rejection, not a native width-conversion error. The latter defensive checks are tested separately.

See the [independent review disposition](../reviews/default-width-2026-10-18/disposition.md) for inspected controls, later author corrections and unqualified cases.

Final [retained evidence](../evidence/nested-milestone/2026-10-18-default-width/README.md) records 162 named Rust tests, the six-window run, six configuration cases, six private regression suites, three focus cases and the two deliberate failing mutations. Final source `87d52d0` includes the later low corrections; the independently reviewed source is `ccfdb61`.
