# Performance targets and verification strategy

Every number below is a proposed target. No qualified FluxWM compositor benchmarks have been recorded; the initial bootstrap does not establish these targets. Choose and publish baseline hardware in phase 1, then review targets after phase 3 without hiding regressions.

## Initial budgets

| Metric | Provisional target | Measurement and caveat |
| --- | --- | --- |
| Main-thread CPU work per active frame | p95 ≤ 2 ms at 1080p/144 Hz baseline | Trace input dispatch through submission, excluding deliberate waits |
| GPU composition | p95 ≤ 3 ms for baseline scene, blur off | GPU timer queries where supported; report missing instrumentation |
| Missed presentation deadlines | < 1% over 5 minutes of scripted scrolling | Use actual presentation data and driver trace; distinguish client stalls |
| Idle | No continuous compositor repaint loop after settling | 60-second static trace; active cursor/capture/media excluded and reported |
| Idle CPU | < 1% of one baseline core median | Scheduler/process counters; publish sampling interval and services |
| Layout update | p95 ≤ 1 ms for 1,000 synthetic windows | Pure model benchmark; does not imply 1,000 real apps are lightweight |
| Visible-scene extraction | Cost tracks visible k, not all n | Compare fixed visible count across 100/1,000/10,000 model windows |
| Lua reload | Finish ordinary config within 100 ms in helper on ordinary reloads | Input/render loop remains responsive during failed/runaway configs |
| Resource growth | Return near steady state after repeated operations | Separate allocator caches, client memory, kernel buffers, and GPU memory |
| Input response | No extra avoidable full-frame queue | Instrument event→submit and use external input-to-photon measurement |

At 60/120/144/240 Hz, frame intervals are approximately 16.67/8.33/6.94/4.17 ms. CPU and GPU work can overlap; do not add arbitrary timing counters and call that input latency. Report p50/p95/p99, worst case, sample count, driver, mode, scale, power profile, and active effects.

## Workload catalog

- W01 idle: two static windows, no cursor motion; verify quiescence and damage suppression.
- W02 strip navigation: 20 windows, keyboard retargets and rapid touchpad direction changes.
- W03 long canvas: 1,000 model groups, fixed visible count, extreme positive/negative pan.
- W04 effects: overlapping translucent windows, moving background, blur/shadow profiles at 1080p and 4K.
- W05 display: 60 Hz + 144 Hz, rotated output, 1.0/1.25/2.0 scales; hotplug during animation.
- W06 fullscreen: accelerated video/game, direct scanout enter/exit, capture enabled/disabled.
- W07 hybrid GPU: offloaded client plus external dGPU output, suspend and dock removal.
- W08 adversarial: malformed buffers, client crash, never-ack client, surface churn, slow IPC reader.
- W09 session: lock while sharing, lock client crash, output hotplug while locked, suspend/resume.
- W10 config: repeated valid/invalid reloads, infinite Lua loop, huge table, duplicate binding.

Each workload needs a seed or scripted event file, expected state assertions, scene settings, run duration, logs, and raw measurements. Store baseline data with exact build revision. Avoid comparing different drivers or power states as if only FluxWM changed.

## Test layers

1. Pure model tests: width constraints, membership, focus, canvas transforms, camera anchoring, rebasing. Property tests generate and shrink command sequences.
2. Animation tests: fake clocks, analytic curve behavior, velocity continuity, suspend gaps, reduced motion.
3. Schema/IPC tests: invalid types, recursion, excessive sizes, framing splits, stale IDs, helper deadline/kill/reap, atomic reload.
4. Client/server integration: synthetic Wayland clients for commit/configure races, popup grabs, buffer lifetimes, selection transfers, fractional scale, explicit sync. Where practical add a [WLCS](https://github.com/canonical/wlcs) integration; an adapter is work, not free conformance.
5. Rendering tests: fixed scenes and reference images per renderer/driver class, tolerances for antialiasing, damage-vs-full-repaint comparisons. Never accept a blank image simply because it is reproducible.
6. Nested end-to-end: launch apps in an isolated display socket, drive commands, inspect scene/focus/screenshots, clean up children.
7. DRM hardware tests: atomic modes, hotplug, mixed-refresh, VRR, multi-GPU, input, VT and suspend. Virtual/nested tests cannot replace these.
8. Security and soak: lock/capture invariants under helper failure, malformed client fuzzing, 24-hour session churn, resource/leak records.

## Hardware and application matrix

Minimum release evidence includes a tested Intel Mesa iGPU, AMD Mesa GPU, and NVIDIA stack with exact driver version recorded. Also test one hybrid laptop and one low-power machine; these may overlap vendor entries. Publish failed/untested combinations explicitly. Use software rendering for deterministic/debug tests only unless separately qualified for production.

Applications: GTK/Qt terminals and dialogs, Firefox/Chromium-class browser and portal screen sharing, Electron editor, accelerated video player, native Wayland game, X11 game/application, clipboard manager, panel, lock client, IME, screen reader, and capture client. Record actual names and versions at test time rather than infer compatibility from toolkit labels.

## Instrumentation

Use structured spans for dispatch, layout, transaction waits, scene extraction, damage, render submission, KMS commit, presentation, capture, and config. Add counters for visible/culled elements, repainted pixels, import/copy failures, scanout eligibility, GPU memory pools, dropped capture frames, outstanding buffers, and frame wakeup reason.

Use CPU sampling/perf and flamegraphs for hotspots, GPU timers/vendor tools for GPU work, DRM tracepoints for presentation where available, and external high-speed camera or latency hardware for input-to-photon claims. Diagnostics overhead must be measured. Trace files can contain sensitive metadata; redact export by default.

## CI and release policy

Phase 1 adds formatting, linting, nested build, and pure tests once Rust code exists. Add protocol/render tests as the features land. Require all supported feature combinations to compile and targeted tests for protocol/lifetime changes. Pin GPU screenshot environment; keep physical-hardware jobs distinct from ordinary hosted CI.

A release candidate needs zero unresolved critical lock/buffer-lifetime/data-loss bugs, passing required phase gates, documented hardware limitations, reproducible build/install instructions, and a clean compatibility matrix. A target miss requires explanation and a concrete decision, not silent relabeling. Unsupported optional HDR/Vulkan paths are acceptable only when the required SDR/GLES desktop passes.

## Evidence-strength and adversarial expansion

The [expanded case catalog](15-edge-cases.md) assigns stimuli, oracles, and required evidence to surface, layout, GPU, output, input, config, security, capture, accessibility, and operations failures. All cases remain `specified`, not passed. [Resource budgets](17-resource-security.md) are admission-policy proposals and must be qualified with legitimate toolkit workloads.

Use [VKMS upstream documentation](https://docs.kernel.org/gpu/vkms.html) to evaluate a virtual-KMS CI job without treating it as physical hardware qualification. Link every future result to exact build/environment/artifacts; absence of an artifact is incomplete evidence. Re-run sensitive cases after optimization fast paths. Provisional p95 numbers cannot certify 240 Hz behavior without measurements at that mode.

W11 captures with blur, protected-associated surfaces and retained lock-redacted streams; record export-safe render cost separately from ordinary rendering. Cases now have introduction and qualification phases plus narrower early variants; only the [phase-gate schedule](evidence/phase-gates.json) controls exit requirements. Runtime case status remains specified.
