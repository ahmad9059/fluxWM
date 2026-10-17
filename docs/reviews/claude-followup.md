# FluxWM documentation remediation: independent follow-up review

The planning decisions are now mostly sound and consistent with each other. But four phase-gate and contract contradictions still need fixing before phase 3, and the worst of them is high severity: the security-context protocol, which the whole sandbox side of the trust model depends on, isn't scheduled before the phase 7 gate that tests it.

## 1. Scope and limitations

- **What I read in full:** `21`, `22`, `13`, `14`, `16`, `17`, `06`, `03`, `04` (from line 25), `07`, `08`, `10`, `18`, `20`, the files `config.lua`, `check-config.lua`, `phase-gates.json`, `edge-cases.json`, `protocol-provenance.json`, `remediation-snapshot.json`, `documentation-checks.json` and `tools/check_docs.py`, and phases 03, 04, 05, 07, 08 and 09.
- **What I read in part:** phases 02 and 06, `00` (line 40 on), `09` (line 34 on), the start of `15`, and targeted searches of `05`, `README.md`, `reviews/README.md`, `decisions/README.md` and `11`.
- **Not read:** `01`, `02`, `12`, `19`, phases 01 and 10, the probe sources, `protocol-inventory.json` and `upstream-sources.json`, apart from search hits.
- **Read and search tools only:**
  - I did not run `check_docs.py`, the Lua checks or the probes.
  - I did not recompute any SHA-256 value. I assume the files I read match `remediation-snapshot.json`, but I haven't verified it.
  - `documentation-checks.json` sits outside the snapshot, so its `"passed": true` can't be tied to the frozen inputs.
- **No upstream sources re-fetched.** Protocol and Smithay claims (xdg-shell version thresholds, syncobj and capture semantics) were judged against the documents and the first review's fetched evidence, not re-checked against source.
- **Plan mode:** it asked for a plan file and an exit call, but this session had no write tool, plan-exit tool or question tool. Nothing was written and no subagents were created.
- **No runtime claims.** No compositor exists, and this review certifies no runtime, hardware or production behaviour.

## 2. CR-01 to CR-15 dispositions

| ID | Status | Basis |
| --- | --- | --- |
| CR-01 | **Still inconsistent** | The scheduling machinery is good: introduced and qualified phases, a feature-first-phase map, variants, and checker coherence checks. Some feature tags are still wrong, so later features still appear in earlier gates (F2). |
| CR-02 | **Still inconsistent** (mechanism) | Model A reads the same in `21`, `17`, `08`, `00` and `16`, with an honest non-isolation claim. But the protocol that tags sandbox connections isn't scheduled (F1), and per-role approval inside the trusted host isn't defined (F3). |
| CR-03 | Resolved design; implementation proof remains | `21` §Capture, `14:76`, `08:29`, `00:49` and CAP-09/10 agree. Some earlier prose still contradicts it (F5). |
| CR-04 | Resolved design; implementation proof remains | Buffer guards held until fence or page-flip completion, the blocked-commit visibility caveat (`14:84`), no extra transaction blocker, and measurable oracles for GPU-04/07/09. Still needs the B04 phase-3 spike. |
| CR-05 | Resolved design; implementation proof remains | Fresh exec per reload, fd/environment allowlist, pidfd tracking, a cumulative budget, coroutines removed, explicit Lua allowlist, patterns compiled in the parent. One leftover: phase 6 still says to test a "coroutine loop". |
| CR-06 | Resolved design | Tagged widths and keys, one ordered output-precedence rule, a physical-pixel suffix, Rust regex syntax, and an executable fixture checker. A few low items remain (§4). |
| CR-07 | Resolved design; implementation proof remains | B20, privacy spreading to related surfaces, notifications excluded by default, neutral background for window capture, and workload W11. Phase 9's task list omits W11 (F5). |
| CR-08 | Resolved design; implementation proof remains | The `22` matrix: version gates, focused and unfocused fullscreen, an opaque backdrop for each fullscreen page, `set_fullscreen(output)` as a preference. B19's phase-7 qualification has no gate case (§4). |
| CR-09 | **Still inconsistent** | B17 exists, but neither phase 3 nor phase 5 has a font or text task, and A11Y-02 needs search, which arrives in phase 8 (F2). |
| CR-10 | Resolved design, with new concerns | Policies for outputs that are powered off or absent, suspend, and lock-screen IME/on-screen keyboard are now written. The powered-off policy (F4) and the IME/keyboard approval rule (F3) are problematic. |
| CR-11 | Resolved | Rows for ext-workspace, virtual keyboard, output power, xdg-dialog, gamma, and foreign-toplevel list vs management, plus phase-7 tasks. Security-context is the exception (F1). |
| CR-12 | **Still inconsistent** | The total was withdrawn (`10:18`), but roadmap row 7 is malformed and still carries "6–12" (F5). |
| CR-13 | Resolved, with stated limits | Distro-package comparison (12 identical files, dmabuf different), mirror labelled unverified, Lua 5.4.9 recorded, cached rerun labelled. Some links still point at the mirror's moving `main` branch (§4). |
| CR-14 | Resolved | Review records exist, the "worker" wording is gone, and the checker's scope and limits sit next to its result. |
| CR-15 | Resolved for the cases listed | SEC-08, GPU-04, LAY-07, OUT-02, A11Y-08 and CAP-07 can now fail. LAY-02 and INP-08 still defer to "documented" behaviour (§4). |

## 3. New high and medium findings

### F1 · High · The protocol that tags sandbox connections is never scheduled, so the B16 phase-7 gate can't be passed
- **Where:**
  - `07-protocols.md:43` lists "Security context/accessibility extensions | **8 evaluation**".
  - `phases/07-desktop.md` tasks 1–7 and line 45 don't include security-context.
  - `phase-gates.json` `feature_first_phase` has no security-context key.
  - SEC-04/05/06 are tagged `lock` and SEC-11 is tagged `desktop`.
- **Why it matters:**
  - `21` §Trust model and `17:7-19` make "tagged sandbox connections are restricted" the only restriction in model A.
  - B16 (`13:63`) qualifies "tagged sandbox denial" in phase 7, and SEC-06/SEC-11 are gated in phase 7.
  - Tagging comes from wp-security-context-v1, which `07` only *evaluates* in phase 8.
  - The checker passes because the tags point at unrelated features.
- **Fix:**
  - Split `07:43` into two rows: "wp-security-context-v1 | 2 design / 7 | listener lifetime, no nested delegation, per-context global filtering" and a separate accessibility-evaluation row at phase 8.
  - Add it to the phase-7 task list.
  - Add `"security_context": 7` to the feature map and retag SEC-04/05/06/11 with it.
  - In `21`, state outright that sandboxes which don't use security-context (older Flatpak, plain bubblewrap and similar) are classified as trusted host. That is a known limitation, not a guarantee.

### F2 · Medium · CR-01 leftovers: mis-tagged cases still pull later features into earlier gates
- **GPU-05 is in the phase 3 gate** ("Remove direct-scanned buffer"), tagged `single_gpu`.
  - Direct scanout is implemented in `phases/09-optimization.md` task 3.
  - `phases/03` has no scanout-of-client-buffers task.
  - **Fix:** add a `direct_scanout: 9` feature, move the full case to phase 9, and give phase 3 a variant `GPU-05:primary-plane`: a compositor-owned framebuffer is held through the page flip that replaces or disables it.
- **A11Y-02 is in the phase 5 gate.** It tests "title in **search and task list**", but search is delivered in `phases/08` task 4. A task list exists only via phase-7 foreign-toplevel or the phase-8 overview.
  - **Fix:** add a variant `A11Y-02:text-fixture` at phase 5 (shaped rendering of a fixture string, with UTF-8-safe truncation), and qualify the full case at phase 8.
- **The phase files don't carry the text work.** `text_ui: 5` gates A11Y-02/03/09, and B17 (`13:64`, `21` §Text/font) requires a font spike in phase 3. But `phases/03` and `phases/05` have no text, font or UI task.
  - `21:55` and `14:51` also promise an accessible on-screen confirmation in phase 5, but no phase-5 task or case covers it. The accessibility adapter itself is phase 8 (B09).
  - **Fix:** add a B17 spike task to phase 3. Add a "shaped text, fallback fonts, glyph atlases, display-change confirmation UI with semantic labels in the data model" task to phase 5. Make clear that the AT-SPI-backed accessibility *qualification* is phase 8.
- **SEC-12 is in the phase 6 gate** and expects "immutable lock invariants", but lock arrives in phase 7.
  - **Fix:** use a variant without lock at phase 6, and qualify the full case at phase 7.

### F3 · Medium · Model A has no way to tell host clients apart, but several contracts require per-role approval
- **Where:**
  - `21:13` and `21:63`: "explicitly approved lock IME/OSK", "no … virtual keyboard injection … bypass[es] current lock ownership".
  - `16:39`: "Restrict input-method binding to a trusted helper".
  - `17:16`: "explicitly selected" accessibility monitor.
  - SEC-07 and SEC-13 oracles.
- **The contradiction:** under model A every unsandboxed same-user process is trusted equally, and `17:19` forbids identifying a role by app_id, title or PID name. Nothing defines how one host virtual-keyboard or input-method client becomes "approved" for the lock surface while another doesn't. So SEC-07 and SEC-13 have no observable condition to pass or fail on.
- **Fix:** pick one of these and write it into `21`, `16`, `17` and the two oracles:
  - **(a) Allow any trusted host client.** Any trusted-host input method or virtual keyboard may deliver input *only into the current lock surface tree* while locked. Document honestly that a host process can type into the locker.
  - **(b) Compositor-spawned helpers.** The lock IME and on-screen keyboard are started by the compositor on a private inherited connection, or on a FluxWM-granted security context, and only that connection is routed while locked.

### F4 · Medium · Waking powered-off outputs to confirm a lock is inconsistent and puts the suspend route at risk
- **Where:** `21` §Lock power (lines 59–61); OUT-09.
- **What's wrong:**
  - **Inconsistent:** with zero outputs, coverage is treated as automatically satisfied ("vacuous"). But an output under compositor control that has been powered off, which also shows nothing, must be powered on just to present a lock frame and then powered off again.
  - **Bad interaction with suspend:** the forced wake overrides the off state the user or idle daemon asked for, and can wake external monitors. Link start-up on a monitor can take seconds, while the suspend contract in the same section waits for confirmation inside a bounded delay. So the documented route may fail qualification by design rather than by bug.
- **Fix:** define "covered" for an inactive output as: its committed atomic state has no desktop framebuffer on any plane, or is disabled. In addition, every path that re-enables it must start from lock or black content, checked with a test-only commit. Under that definition, confirm the lock without waking the output. Keep the forced active commit only as a fallback for backends where this can't be guaranteed, under B08. Then make OUT-09's oracle "no wake caused by lock; first framebuffer scanned out after wake is lock or black". Whether a given KMS driver allows this needs backend verification.

### F5 · Medium · Earlier prose still contradicts the decisions
- `phases/07-desktop.md:13` lists "capture **revocation**" under the session-lock task, which contradicts B18's retained black sessions. Change it to "capture lock-redaction; terminal stop only on revoke or source loss".
- `08-desktop-security.md:19` says to "reject … capture … while locked". No rule covers a *new* capture session requested while locked.
  - **Fix:** state whether new sessions are created in the redacted (black) state or immediately stopped, and add a case for it.
- `10-roadmap.md:13`: the phase 7 row's columns are shifted. "Dependencies" contains "Not estimated…" and "Scheduling evidence" still says "**6–12**", the estimate `10:18` says was withdrawn. The real dependency (3–6) is gone.
- `phases/09-optimization.md:16` runs "W01–W10", but B20 (`13:67`) requires W11 in phase 9.
- `phases/06-lua-ipc.md:25` says to test a "coroutine loop", but `21:45` removes coroutines. Change it to "coroutine library absent".
- `14:68` still says "Choose a documented policy" for powered-off outputs. `16:61` still says the font stack is selected "during phase 5", while B17 requires the phase-3 spike. Both should point to `21`.

## 4. Low items, briefly
- **Moving links:** `07:3` calls the mirror's `blob/main` copy "authority". `04:23,25` and `08:19` also link to `main`. Use pinned links and the provenance caveat. `protocol-provenance.json` checked package ownership for only one of the 13 files.
- **Window-state coverage:**
  - B19's phase-7 native/X11 qualification has no gate case: SURF-09 is phase 8 and LAY-09 is phase 4. Add a variant `SURF-09:strip-native` at phase 7.
  - No phase is given for when minimize first exists, which affects `wm_capabilities`.
  - `22:28` should say whether `invalid_surface_state` is raised only for buffers committed after the maximized configure was acknowledged.
- **Key bindings:** `"Super+Space"` doesn't match the case-sensitive xkb keysym name `space`. The normalization for Shift plus a letter is undefined. The fixture checker doesn't validate keysym names.
- **Privacy rule:** `capture = "allow"` is in the checker's enum, but its meaning against the "sticky deny" in `16:79` is undefined.
- **Vague oracles:** LAY-02 and INP-08 still defer to "documented" policy. GPU-07 and GPU-07:implicit at phase 3 are effectively the same case.
- **Cosmetic tags:** CAP-06/07/08 (clipboard, X11, IME) are tagged `capture`. It does no harm to scheduling, but it's misleading.
- **Helper fds:** the helper relies on every library setting close-on-exec. Closing inherited fds explicitly in the child before exec (for example with `close_range`) would be sturdier.

## 5. Readiness verdict
- **Documentation:** a large improvement. The design decisions for trust (model A), capture-on-lock, buffer guards, helper containment, schema v1, the window-state matrix and the privacy graph are concrete and mostly consistent. But it is **not yet internally consistent**: F1 makes the B16 phase-7 gate impossible to pass, and F2 still puts later features in the phase 3 and 5 gates. Every fix above is a documentation edit; none needs new research.
- **Phase 1:** ready to start.
- **Phase 2:** can start, as long as the F1 fix adds security-context to the B16 design scope before any privileged globals are registered.
- **Phase 3 and later:** start after the F1, F2 and F5 edits. F3 and F4 must be decided before the phase-7 lock work is designed in detail.
- **Still needing implementation evidence:** B04, B15, B17, B18, B19, B20 and B08, plus every case, all of which remain "specified".

None of this certifies production readiness, runtime behaviour or hardware compatibility.
