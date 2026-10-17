# Claude finding disposition

The [original report](claude-review.md) is preserved unchanged, with [raw result metadata](claude-review-result.json). The CLI reported successful completion using `claude-opus-5-5` as the review model; metadata also records its WebFetch summarization model. No subagents were spawned and no write/command tools were available. This is independent document review, not runtime qualification.

The report's opening says “seven high-severity findings,” but its individual entries label CR-01–03 High and the rest Medium/Low. This disposition follows individual finding IDs and substance rather than that inconsistent count.

| Finding | Disposition and concrete change | Remaining implementation proof |
| --- | --- | --- |
| CR-01 | Replaced single case phase with introduced/qualified phases, feature dependencies, seven narrow early variants and exact phase-gates mapping; checker verifies row and scheduling coherence | Every runtime case is still specified |
| CR-02 | Selected compatible trusted-host/restricted-sandbox model A consistently for Wayland, IPC, config and activated helpers; added B16 and SEC-09–12 | Helper activation, sandbox tagging and deny-path integration |
| CR-03 | Retain capture session with opaque black on lock; terminal stop only for true revoke/source loss; explicit source and destination leases/completion; CAP-09/10 and B18 | GPU destination destruction/fence and browser lock-retention tests |
| CR-04 | Explicit Buffer guard per consumer through nonblocking fence/page-flip completion; blocked-commit visibility and transaction-blocker decisions; measurable unused-release/GPU oracles; SHM explicit-sync case | Concrete pinned backend implicit readiness spike B04, explicit/multi-GPU phase 9 |
| CR-05 | Fresh exec helper per reload, fd/environment allowlist, parent handle/deadline/reap, limits and Lua allowlist; cumulative advisory instruction budget; parent-side native pattern compilation | B15 actual supervisor, limits and adversarial embedded-runtime tests |
| CR-06 | Tagged widths/keys, single ordered output precedence, exact selector table, physical snap suffix, Rust regex dialect and metadata privacy reevaluation; executable fixture checker | Future hardened Rust schema/helper and regex/hardware validation |
| CR-07 | Privacy propagation to associated surfaces/derived UI; unknown associations conservatively excluded; neutral window-capture background; W11 cost workload and B20 | Pixel/effect dependency validation across fast paths |
| CR-08 | Added version-gated XDG state/capability/bounds/backdrop matrix, focused/unfocused fullscreen policy and B19 with native/canvas cases | Toolkit/bridge state traces and fullscreen alpha fixtures |
| CR-09 | Added B17 minimal font spike phase 3/full UI phase 5, fallback/emoji/cache contract; early CLI-only timed display preview | Font-stack selection/build/raster/accessibility evidence remains explicitly open |
| CR-10 | Explicit powered-off-output safe black activation/confirmation, no-display/inactive-seat policy, output-power row, suspend contract, lock IME/OSK, no lock UI capture | B08 physical presentation, suspend timing and lock input tests |
| CR-11 | Added ext-workspace, virtual keyboard, output power, xdg-dialog, optional gamma and management/list separation; explicit phase-7 tasks | Exact protocol/companion versions and real taskbar/idle/IME proof |
| CR-12 | Withdrew unsupported 38–76 week forecast; per-contract estimate and lab/CI/WLCS/latency/accessibility ownership required; B21 | Real staffing/access/cost estimates |
| CR-13 | Compared all 13 XML files with installed distro package: 12 identical, one explicit DMA-BUF clarification difference; label mirror provenance unverified; record bundled Lua 5.4.9 and cached rerun | Authenticated selected upstream/distro build inputs in B01; no signature audit claimed |
| CR-14 | Saved actual review and this disposition; removed worker wording; checker scope explicit next to passed, now checks phase and row consistency | No assertion of complete production consistency from a script pass |
| CR-15 | Replaced vague oracles with observable release/order/identity/focus/greeter/assistive assertions | Destructive cases require isolated supported session evidence |

Design ambiguities are addressed in [review resolutions](../21-review-resolutions.md) and [window-state matrix](../22-window-state-matrix.md). Open implementation choices are not labeled tested or production-ready. A follow-up review checks the frozen remediation snapshot; it is recorded separately when complete.
