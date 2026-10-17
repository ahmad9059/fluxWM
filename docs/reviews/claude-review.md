# FluxWM planning documentation: independent review

**Verdict:** The documents are honest about their status and the source pinning is good. But they are not yet an internally consistent plan for building the whole compositor. Phases 1–2 can start. Several later phase gates can't be passed as written, and the trust model for privileged clients contradicts itself. I found no critical errors and seven high-severity findings.

This is a review of documents only. It does not certify any runtime behaviour, compatibility or production readiness. No compositor exists and I ran nothing.

---

## 1. What I reviewed, what I fetched, and limitations

**Files read:** `README.md`, every file in `docs/` (00–20), `docs/README.md`, `decisions/README.md`, all ten phase files, the review records, `examples/config.lua`, and everything in `docs/evidence/`. That includes the source manifest, protocol inventory, the 80-case JSON, the preflight record, both Lua probes, the mlua probe crate and its result, and `documentation-checks.json`. I also read `tools/check_docs.py` but did not run it.

**The repository changed while I was reviewing it.** These appeared after my first pass:
- `docs/20-implementation-contracts.md`
- the mlua probe (`docs/evidence/probes/mlua-hook/`) and its result files
- `tools/check_docs.py` and `docs/evidence/documentation-checks.json`

`06-lua.md`, `09`, `13`, `16`, `17`, `18`, `phases/04` and `README.md` were also edited. I re-read every changed file I noticed, and the findings below reflect that later state. Files I read only once may have changed again since. Future reviews should be run against a frozen snapshot, for example a list of file hashes.

**Sources fetched with WebFetch** (all at the commits pinned in `upstream-sources.json`):
- Protocol XML from the `wayland-mirror` copy: `ext-session-lock-v1`, `linux-drm-syncobj-v1`, `ext-image-copy-capture-v1`, `xdg-shell`.
- Smithay `023b78b`: `src/wayland/drm_syncobj/mod.rs`, `src/wayland/drm_syncobj/sync_point.rs`, `src/backend/renderer/utils/wayland.rs`.
- niri `45739bc`: `docs/wiki/Accessibility.md`.
- The GitHub pages for `wayland-mirror/wayland-protocols` and `niri-wm/niri`, to check source identity.

**Limitations:**
- WebFetch returns content summarised by a model, not raw bytes. I did not check any SHA-256 hash, and some "quotes" came back partly paraphrased.
- Not fetched: the security-context, image-capture-source and dmabuf XML; Smithay's session_lock, security_context and image_copy_capture modules; the xdg-desktop-portal-wlr, xwayland-satellite and mlua sources; the kernel docs.
- I didn't run the probes or the doc checker. Claims about probe results come only from the recorded files.
- Plan mode asked for a plan file, but this session had no write tool, so nothing was written anywhere.

---

## 2. Findings

Types used below:
- **Error:** the documents contradict themselves or misstate a fact.
- **Decision:** a choice the documents leave open that blocks implementation.
- **Improvement:** optional.

### CR-01 · High · Error: test-case phases contradict feature phases, so some phase gates can't be passed
- **Where:** `evidence/edge-cases.json`; `phases/03-gpu-backend.md:43`, `phases/05-animation.md:43`, `phases/04-scrolling-layout.md:43`, `phases/08-canvas.md:43`; `07-protocols.md:14,19-22`; `phases/03-gpu-backend.md:33`.
- **What's wrong:** Phase 3's gate requires GPU-01–08 and OUT-01–08, but several of those test features that arrive later:

  | Case | Needs | Feature arrives in |
  | --- | --- | --- |
  | GPU-03 | syncobj protocol error | Phase 9 (phase 3's own notes say explicit sync isn't advertised before then) |
  | GPU-06 | multi-GPU | Phase 9 |
  | GPU-08 | blur and capture | Blur in phase 5/9, capture in phase 7 |
  | OUT-01 | capture | Phase 7 |
  | OUT-04 | confirmation UI with text, output config protocol | Text in phase 5, output config in phase 9 |
  | OUT-06 | lock | Phase 7 |

  Phase 5 has the same problem:

  | Case | Needs | Feature arrives in |
  | --- | --- | --- |
  | INP-02 | lock | Phase 7 |
  | INP-04 | pointer constraints | Phase 7 |
  | INP-05 | canvas zoom | Phase 8 |
  | INP-07 | tablet | Phase 7 |

  Other mismatches:
  - The JSON assigns LAY-04/05 (canvas) to phase 4, but phase 8 claims them.
  - Phase 5 requires A11Y-02/03, but the JSON says phase 8.
  - `check_docs.py` only checks that each phase number is between 1 and 10, so it can't catch any of this.
- **Fix:**
  - Give each case two fields, `introduced_phase` and `qualified_phase`.
  - Write variants for the phase 3 gate that use implicit-sync dmabuf only (GPU-01/02/04/05/07).
  - Move the lock, capture, canvas and multi-GPU cases to the phases where those features exist.
  - Add a checker rule: a case's phase must be at or after the first phase of every protocol or feature it exercises.

### CR-02 · High · Decision: the trust rules for same-user and sandboxed clients contradict each other
- **Where:** `17-resource-security.md:11,17,19`; `08-desktop-security.md:43`; `06-lua.md:22,36`; `07-protocols.md:66`.
- **What's wrong:**
  - **Wayland is strict.** Ordinary clients get no raw capture, lock ownership or global input. Helpers are recognised only by how they were launched, e.g. an inherited private connection, and other connections are denied by default.
  - **That breaks the usual companion tools.** They connect through `WAYLAND_DISPLAY` and are started by other daemons:
    - lockers started by idle daemons
    - xdg-desktop-portal backends, which D-Bus or systemd activates (so there's no inherited connection to check)
    - IME daemons
    - data-control clipboard tools
    - taskbars
  - **IPC and config are loose.** Any process running as the same user can `spawn`, `quit` or change config over IPC. It can also edit `config.lua`, including `security.capture` and spawn bindings, which is reloaded automatically. Sandboxed apps with home-directory access can do that too.
  - So the strict Wayland rule adds little protection against same-user processes and mainly costs compatibility.
- **Fix:** Choose one model and apply it to Wayland, IPC and config alike, then add it as blocker B16 (design in phase 2, closed in phase 7). Either:
  - **(A)** Trust unsandboxed same-user clients for a listed set of privileged protocols, and restrict clients tagged by security-context (sway/niri-style).
  - **(B)** Go strict everywhere. Gate IPC mutations the same way, make security fields in config require confirmation instead of live reload, and provide a supported pre-authorised launch path (for example `fluxctl run --role capture -- …` creating a scoped socket). This option also needs a way to identify a D-Bus-activated portal backend.

  Add test cases for: a locker started by an idle daemon, a D-Bus-activated portal, data-control clipboard, a sandboxed (Flatpak) client tagged by security-context, and a config file edited to weaken policy.

### CR-03 · High · Error + Decision: what happens to screen capture on lock is contradictory, and the capture output buffer isn't tracked
- **Where:** `14-state-machines.md:65,76`; `08-desktop-security.md:29`; `00-product.md:49`; `09-performance-testing.md:32` (W09); CAP-01.
- **What's wrong:**
  - The documents say three incompatible things:
    - `14:76`: on lock, stop or fail the capture source.
    - `14:65`: on unlock, re-evaluate capture grants, which assumes they survived.
    - `00-product.md:49`: the example journey shares a window, then locks and resumes.
  - The capture protocol has no paused state. I checked the XML: `stopped` means "the capture session has stopped and is no longer available", and the client "should destroy the session".
  - So stopping on lock ends a browser share permanently. The alternatives are holding back frames or sending opaque frames. Both are allowed by the protocol and both have tradeoffs.
  - Separately, the compositor writes into a buffer the capture client supplies, and `ready` means the buffer can be reused. So `ready` must not be sent until the GPU write has finished. The buffer ledger in 14 only tracks *source* buffers being read; it doesn't cover these destination buffers or a client destroying one mid-copy.
- **Fix:**
  - Decide the policy. I'd recommend: keep the session across lock, produce no new desktop pixels (hold frames or send opaque ones), and use `stopped` only for real revocation or loss of the source.
  - Define exactly what CAP-01 accepts for frames already in flight when the lock happens.
  - Add destination-buffer entries to the ledger, plus a test case where the client destroys the buffer during a copy.

### CR-04 · Medium · Decision: buffer-release mechanics under Smithay are left unspecified
- **Where:** `14-state-machines.md:7-9,35-45`; `04-rendering.md:63`; GPU-02/04/07.
- **What the docs get right:** They're correct that a layout timeout must not bypass the acquire point, and that a buffer that was never read may be released before its acquire point signals. The syncobj XML says "compositors may release buffers without ever reading from them" and "clients cannot rely on the release point being always signaled after the acquire point".
- **What's missing:**
  - **Release happens on drop.** At the pinned Smithay commit, `InnerBuffer::drop` sends `wl_buffer.release` and signals the release point from the CPU straight away. Smithay's own drm_syncobj module docs describe the release fence as signalled when all references are dropped. Also, `merge_into` signals a replaced release point, and the render paths never check the acquire point. So FluxWM's buffer ledger has to *own* the Smithay `Buffer` clones and drop them only after the GPU fence or page flip completes. That needs an asynchronous fence wait for every submission, and the docs don't specify one.
  - **Layout and acquire readiness arrive together.** The acquire blocker (`DrmSyncPointBlocker`) holds back the whole commit, including the acknowledged configure and new size. So in Smithay, "layout ready" can't be seen before "acquire ready".
  - **Undecided:** whether transactions hold back commits that are already ready (niri-style), and whether implicit-sync dmabuf commits also get blockers.
  - **Not mentioned:**
    - Signalling a timeline point also signals every earlier point on that timeline (the XML recommends one timeline per buffer).
    - `wl_buffer.release` becomes undefined on syncobj surfaces.
    - Smithay raises `unsupported_buffer` for non-dmabuf (e.g. SHM) buffers.
  - GPU-04's oracle, "retire safely per protocol", can't be measured.
- **Fix:** Write down the guard mechanism (hold `Buffer` until the render fence fd or page flip) and both blocker decisions. Make the oracles concrete:
  - **GPU-04:** a replaced buffer that was never imported has its release signalled within one dispatch, with zero recorded readers.
  - **GPU-07:** with GPU work deliberately delayed, the release timestamp comes after the render fence timestamp.

  Add an SHM-on-syncobj-surface case.

### CR-05 · Medium · Error + Decision: config helper limits contradict each other and are under-specified
- **Where:** `17-resource-security.md:32-35,46`; `06-lua.md:28-30,36,65`; `14-state-machines.md:80`; `decisions/README.md:57`.
- **What's wrong:**
  - **Where are patterns compiled?** `17:35` says on the helper; `06:65` says the parent compiles them.
  - **Is the helper spawned for each reload or kept running?** The docs are ambiguous. If it's spawned per reload, it starts after the compositor already holds DRM-master, libseat and input file descriptors. Nothing specifies spawning a dedicated binary via exec, closing inherited descriptors, `no_new_privs`, CPU/address-space/file limits, or using a process handle (pidfd) to track it.
  - **The "1 million instructions" budget is undefined.** It doesn't say how instructions are counted, whether the limit stays tripped once hit, or whether it covers coroutines (this needs checking under Lua 5.4).
  - **The probes show something narrower than the design.** Both probes use a hook that raises an error every 1,000 instructions. That isn't the proposed budget.
  - **The allowed library set isn't listed.** `load` with binary chunks, `string.dump`, `debug` and `collectgarbage` aren't addressed by name.
  - `decisions/README.md:57` still says "Worker".
- **Fix:**
  - Pick one compile location: the parent, with bounded regex size limits.
  - Specify how the helper is spawned and what fds, environment and limits it gets.
  - Publish an exact allowlist of Lua functions, with `load` restricted to text mode or removed.
  - Define the budget semantics.
  - Add a "security-sensitive" class to the reload diff that needs confirmation (ties to CR-02).

### CR-06 · Medium · Error: the config example contradicts the schema
- **Where:** `examples/config.lua:10-13,21-24,43-57,61`; `03-layout.md:9,45,55`; `06-lua.md:16,24,51,53`; `05-animation-input.md:29`.
- **What's wrong:**
  - **Output matching:** the documents give two different precedence rules — rule order (`06:16`) and "specific beats wildcard" (`06:53`, plus the example's comment). The example's `match = "eDP-1"` uses a bare connector name, which `03:55` and `14:53` themselves say is the weakest identity. There's no EDID form.
  - **Widths:** `03:9` defines three kinds of width (fixed, fraction of viewport, preset index). The example just uses bare numbers like 0.5, which can't distinguish them.
  - **Units:** `snap_distance` is physical pixels in `03:45` but logical pixels under `06:24`.
  - **Key bindings:** `05:29` distinguishes keycode bindings from keysym bindings, but `key = "Super+H"` can't say which.
  - **Patterns:** the pattern syntax (regex or Lua patterns) is never specified.
  - **Privacy rule:** `capture = "deny"` matches on app_id, but rules are only re-evaluated on *title* changes (`06:51`). A client can set or change its app_id after mapping.
- **Fix:** Define schema v1 now with typed tags:
  - widths: `{fraction=}` or `{px=}`
  - outputs: `{connector=, make=, model=, serial=}` with one precedence rule
  - key bindings: `{sym=}` or `{code=}`
  - patterns: a named dialect

  Also re-evaluate privacy rules on every metadata change, and make the doc checker validate the example against that schema.

### CR-07 · Medium · Decision: privacy through visual effects is recognised, but the mechanism is incomplete
- **Where:** `04-rendering.md:65`; `16-integration-contracts.md:22-27`; CAP-04; `09-performance-testing.md:29`.
- **What's wrong:** It's good that the docs already reject "black box over the private window" as insufficient. But:
  - **Marking:** the only way to mark a window private is an app_id rule (see CR-06 for why that's leaky).
  - **Other clients' surfaces:** IME candidate popups and notifications showing private text belong to other clients, so redaction keyed on the window misses them.
  - **On-screen compositor content:** overview thumbnails, closing snapshots, the magnifier and cursor surfaces all appear in an *output* capture. The internal-vs-export split in `16:27` doesn't cover what's simply on screen.
  - **Cost:** rendering a second, redacted composition while sharing isn't in any performance budget. W06 is only "capture on/off".
  - **Window capture:** it's undecided what a captured window shows behind it when it uses blur.
- **Fix:**
  - Add a rule for building the capture scene: privacy classes spread to child and associated surfaces (popups, subsurfaces, IME popups tied to the focused text input, snapshots, thumbnails), and effects only sample from the capture scene.
  - Render window captures without any desktop-sampled background.
  - Add a workload for sharing with blur on and a private window open.

### CR-08 · Medium · Error + Decision: fullscreen, maximize and window-state semantics leave out protocol obligations
- **Where:** `03-layout.md:35`; `20-implementation-contracts.md:77,97`; `07-protocols.md:50`.
- **What's wrong:** Maximize is now defined (`20:77`). Checked against the xdg-shell XML, these obligations are missing:
  - **Maximized:** the client must obey the size or the protocol raises `invalid_surface_state`.
  - **Fullscreen:** a surface smaller than the output must be centred. If the surface isn't opaque, the compositor must ensure other screen content isn't visible below it. That clashes with the fullscreen window keeping its place in the scrolling strip while neighbours are visible during a pan.
  - **Unfocused fullscreen:** `03:35` says fullscreen uses the whole output "when focused", but doesn't say what state the *client* is told when focus moves away.
  - **`set_fullscreen` output argument:** how it's handled isn't specified.
  - **`wm_capabilities`:** must be sent before the first configure and must accurately cover minimize, which now exists.
  - **Unused window states:**
    - `suspended` is never used, even though it's the protocol's way to throttle off-screen canvas windows.
    - `configure_bounds` is never mentioned.
    - The `tiled_*` and `constrained_*` states are never mentioned.
- **Fix:** Add a table mapping each layout state to what the client is sent. Layout states: tiled, stacked, hidden tab, floating, maximized, fullscreen focused, fullscreen unfocused, minimized, off-screen canvas, overview. For each, list the xdg states, bounds, suspended flag, capabilities and backdrop. Add matching test cases.

### CR-09 · Medium · Decision: the text rendering stack is chosen too late and has no blocker
- **Where:** `16-integration-contracts.md:59-63`; OUT-04 (phase 3); `20-implementation-contracts.md:45`; `phases/05-animation.md:41`.
- **What's wrong:**
  - **Text is needed before phase 5.** The text and font stack is chosen in phase 5 with no blocker ID, but on-screen text is needed earlier: the display-change confirmation (a phase 3 case) and startup feedback.
  - **Not covered at all:**
    - font discovery and the fallback fonts guaranteed in a minimal session
    - colour emoji in GLES
    - glyph caches per output scale and per overview zoom
    - text under blur or transforms
- **Fix:** Add a blocker for the text/font stack, with a minimal deadline in phase 3 and full closure in phase 5. Give phase 3 a confirmation flow that needs no text, such as a timed revert.

### CR-10 · Medium · Decision: session lock edge cases are left open
- **Where:** `08-desktop-security.md:15,21`; `14-state-machines.md:55-70`; OUT-06; SEC-07.
- **What's right:** Checked against the XML, the core rules are correct:
  - The `locked` event is sent only after a locked frame is presented on every output.
  - All outputs are blanked opaque.
  - The lock client dying must not unlock.
  - A new client may take over the lock, at the compositor's discretion.
- **What's open:**
  - Who may bind the lock manager at all (see CR-02).
  - No proposed policy for outputs that are powered off, where "presented" can't happen.
  - The output power-management protocol idle daemons use to turn screens off is missing from the protocol matrix.
  - Suspend timing: logind gives only a short window before sleep, so the compositor should blank every output itself when it's told sleep is coming, whether or not the locker is ready.
  - No positive requirement for IME or an on-screen keyboard on the lock screen.
  - No rule on whether the lock screen itself can be captured.
- **Fix:** Write these policies into 14 and add test cases for each.

### CR-11 · Medium · Error: the protocol matrix is missing protocols the declared companion session needs
- **Where:** `07-protocols.md` matrix; `examples/config.lua:63-67`; `phases/07-desktop.md:11-17`.
- **What's missing:**
  - `ext-workspace-v1`: the example starts waybar, which has no FluxWM module, so it can't show workspaces without this.
  - virtual-keyboard: used by IME frameworks, on-screen keyboards and VNC.
  - output power management: idle daemons use it to switch displays off.
  - `xdg-dialog` (for the modal policy in `16:69`).
  - foreign-toplevel *management*, i.e. taskbar actions. The list protocol alone doesn't provide them.
  - gamma control (optional).

  Also, the matrix puts tablet, xdg-foreign and pointer gestures in phase 7, but phase 7's task list doesn't include them.
- **Fix:** Add these rows with owner phases and align phase 7's task list with the matrix.

### CR-12 · Medium · Error: effort estimates are unrealistic
- **Where:** `10-roadmap.md:7-18`.
- **What's wrong:**
  - Phase 7 is given 6–12 person-weeks for roughly 20 protocol families, plus lock and suspend, a possibly custom portal backend with PipeWire, the X11 route, and session integration.
  - Phase 9 is given 5–10 person-weeks for explicit sync, qualification on three vendors plus hybrid laptops, direct scanout/VRR/output management, and effects.
  - The hardware lab, latency camera, VKMS CI and WLCS adapter aren't budgeted at all.
  - The documents do caveat these as "not researched promises" and plan to re-estimate after phases 3 and 7, but the total of 38–76 person-weeks will still be read as a forecast.
- **Fix:** Break estimates down per contract, show the total as unknown until after phase 3, and budget for test infrastructure and hardware.

### CR-13 · Low · Error: source identity and evidence provenance are incomplete
- **Where:** `01-research.md:46`; `18-source-findings.md:15,31`; `protocol-inventory.json`; the probe's `Cargo.lock`; `mlua-probe-result.json:7`.
- **What's wrong:**
  - **The protocol XML comes from an unofficial mirror.** `wayland-mirror/wayland-protocols` shows no description and no information on how it's synced. The hashes prove the mirror's bytes, not that they match freedesktop. `01:46` admits this, but `18` and the protocol inventory treat the mirror as authoritative.
  - **The two Lua probes may have run different 5.4 patch levels.** The mlua probe uses the bundled `lua-src 551.0.2` and only prints "Lua 5.4", while the standalone probe used 5.4.9.
  - **The two probe records disagree slightly.** The probe README's command omits `--offline`, which the recorded run used. The 0.01 s build time also shows that run reused an existing build.
  - `niri-wm/niri` checks out as a non-fork repo with about 28.5k stars, so that identity looks fine.
- **Fix:**
  - Map each XML file to a freedesktop commit, signed release tarball or distro package, and record both hashes.
  - Record the bundled Lua patch level.
  - Label the probe result as a cached rerun.

### CR-14 · Low · Error: some status claims are inflated or stale
- **Where:** `README.md:9`; `reviews/README.md:5`; `decisions/README.md:57`; `documentation-checks.json`.
- **What's wrong:**
  - The README points to "Claude's findings and their disposition", but the review records contain none yet.
  - `decisions/README.md:57` still says "Worker".
  - `"passed": true` from the doc checker is easy to read as consistency. It only checks links, structure and hashes; it doesn't compare the case JSON with the Markdown or check phase coherence.
- **Fix:** Fix the wording and state the checker's scope next to the "passed" result.

### CR-15 · Low–Medium · Error: some test oracles can't fail
- **Where:** `15-edge-cases.md`: SEC-08, GPU-04, LAY-07, OUT-02, A11Y-08, CAP-07.
- **What's wrong:** Their expected results defer to "documented" behaviour or "per protocol", and that documentation doesn't exist yet, so no observation could make them fail.
- **Fix:** Point each one at a decision record and a measurable assertion. For example, SEC-08: after SIGKILL of the compositor while locked, the next visible frame on every output is the greeter or VT, verified by camera or KMS capture.

---

## 3. Missing production requirements, and whether blocker ownership is enough

**Missing requirements:**
- A trust-model decision covering Wayland, IPC and config together (CR-02).
- A capture-on-lock policy (CR-03).
- A text/font stack blocker (CR-09).
- An xdg window-state mapping (CR-08).
- Protocols for display power management, workspaces for panels, and virtual keyboard (CR-11).
- A rule for building the privacy-filtered capture scene (CR-07).
- A defined helper spawn/limits contract (CR-05).
- Owners for the test infrastructure and hardware lab.

**Blocker ownership is not sufficient yet:**
- B01–B15 have roles but no named people, which the documents acknowledge.
- B05 is "every phase" with no single closure point.
- B04 and B13 are claimed by several phases with no closure criterion for each.
- There's no blocker ID for trust policy, text, capture-on-lock or window-state mapping. `13:55` mentions the trust policy but gives it no ID or evidence requirement.
- Because of CR-01, test cases are tied to phases in a way that makes some gates impossible.

## 4. Verdict

- **Documentation quality:** high in honesty and structure. Status is clearly labelled, proposals are kept separate from upstream facts, sources are pinned with hashes, and case statuses are kept at "specified".
- **The technical fundamentals are mostly sound.** Separating acquire from layout readiness, allowing release before acquire for unread buffers, and the core lock semantics all match the upstream XML I checked.

**Readiness:**
- **Phase 1:** ready to start (toolchain, dependency pinning, licence, the nested sample).
- **Phase 2:** can start once CR-02 is decided, because the documents themselves require the privileged-global model before any globals are registered.
- **Phase 3 onward:** not ready until CR-01, CR-03 and CR-04 are fixed and CR-08 and CR-09 have decisions. Otherwise the gates either can't be passed or leave key lifetime and privacy behaviour undefined.

Again: this verdict covers the documents only, not any runtime behaviour, hardware compatibility or production readiness.
