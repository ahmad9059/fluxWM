# FluxWM final focused consistency review

**Verdict:** F1–F5 are all fixed in the documents I read. I found no new or remaining high- or medium-severity contradictions. The documentation is ready to start phased implementation at phase 1. Only a few low-severity wording fixes are left.

## 1. What I checked, and what I didn't

- **Read in full:** `claude-followup.md`, `followup-disposition.md`, `final-review-snapshot.json`, `21-review-resolutions.md`, `22-window-state-matrix.md`, `10-roadmap.md`, `phase-gates.json`, `phases/05` and `phases/07`.
- **Read in part:**
  - `17` lines 1–67.
  - `16` lines 35–84.
  - `14` lines 53–84.
  - `08` lines 13–32.
  - `13` lines 28–69.
  - `06` lines 40–76.
  - `config.lua` lines 40–70.
  - `00` lines 43–51.
  - `phases/02` and `phases/03` tasks plus the follow-up notes.
  - `protocol-provenance.json` header and limits.
  - `07` lines 1–8 plus search hits.
  - The relevant `edge-cases.json` entries.
- **Searched only:** `15`, `03`, `05`, `01`, phases 06, 08, 09 and 10, and the phase dependency lines.
- **Not read:** `frozen-input-check.json`, `check_docs.py`, the probes, and the rest of the tree.
- **Nothing run or recomputed.** I ran no checks, Lua or probes and recomputed no SHA-256 values. I assumed the files match `final-review-snapshot.json`; the author's recompute is what actually confirms that. I didn't count all 94 cases. I did count the variant rows in `15`, and there are 11.
- **No sources fetched.** Protocol, Smithay, KMS and driver claims, such as whether a driver allows lock coverage without waking a powered-off output, were not checked against upstream.
- **No plan file written.** Plan mode asked for a plan file and an exit call, but this session had no write, plan-exit or question tool.
- **No runtime, hardware or production readiness is certified.**

## 2. F1–F5

| Finding | Status | Where it's fixed |
| --- | --- | --- |
| F1: security-context not scheduled | **Resolved** | `07:43` now reads "2 design / 7 implementation", with accessibility split into its own phase-8 row at `07:44`. `phases/02:45` covers B16 design. `phases/07:49` has the implementation task. `phase-gates.json` maps `security_context: 7`. SEC-04/05/06/11 now carry `security_context`. `21:9` states the limitation that untagged sandboxes count as trusted host. |
| F2: later features in early gates | **Resolved** | GPU-05 is now phase 9 (`direct_scanout`), with `:primary-plane` at phase 3 (`phases/03:47`). A11Y-02 is now phase 8 (`search_ui`), with `:text-fixture` at phase 5. B17 is in phase 3 and phase 5 (`phases/05:47`: data model in phase 5, AT-SPI in phase 8). SEC-12 is now phase 7, with `:host-control` at phase 6. The JSON and Markdown rows agree. |
| F3: no way to approve one host input client over another | **Resolved (option a)** | `21:13`, `21:63`, `16:39` ("no unspecified per-program approval"), `17:16`, and the SEC-07/SEC-13 oracles now agree: any authenticated trusted-host input method or on-screen keyboard may type only into the current lock tree. Tagged sandbox injection is denied, and compositor-wide key monitors get nothing. |
| F4: waking powered-off outputs to lock | **Resolved as a design contract** | `21:59`: no wake caused by lock; a disabled or plane-cleared state proven by the backend counts as covered; every enable path commits black or lock content first; a qualified active-black fallback is allowed, or lock is refused; B08 is responsible for proof. `14:68` and the OUT-09 oracle match. Whether real drivers can do this is still for B08, which is ordinary planned proof. |
| F5: leftover contradicting prose | **Resolved** | `phases/07:13` now says lock-redaction, with terminal stop only on revoke or source loss. New capture grants while locked are denied (`08:19`, `21:69`, CAP-12 at phase 7). Roadmap: next item. `phases/09:16,47` now include W11. `phases/06:25` tests that the coroutine library is absent. `14:68` and `16:61` point to `21`. |

**Roadmap (`10-roadmap.md`):** the table now has four well-formed columns. Phase 7's dependency is "3–6", which matches `phases/07:3`. Every row says "Not estimated". All phase-file dependency lines match the table.

**Low items from the follow-up:**
- **Fixed:**
  - Pinned XML links (no `blob/main` links remain).
  - Package ownership recorded for all 13 XML files.
  - The `SURF-09:strip-native` variant at phase 7.
  - Minimize implemented from phase 4 (`22:42`).
  - Maximized validation scoped to a new buffer after an acknowledged configure (`22:28`).
  - Canonical `space` key name and explicit Shift (`06:69,75`; `config.lua:49,52`).
  - `allow` can't clear a sticky deny.
  - Concrete LAY-02 and INP-08 oracles.
  - CAP-06/07/08 retagged.
- **Still open:** `GPU-07` and `GPU-07:implicit` remain near-duplicates at phase 3. This wasn't claimed as fixed and is harmless.

## 3. New or remaining high/medium contradictions

**None.**

These low-severity fixes are optional before phase 2:

1. **Helper fd closing.** `21:41` is cited as the "exact" spawn spec (`17:66`, `20:40`) but still only says fds are "close-on-exec". The explicit closing of every fd outside the allowlist before exec appears only in `17:46`. **Fix:** copy the `17:46` sentence into `21:41`.
2. **Sticky deny vs. rule order.** `06:51` says later rules win for the same field. `16:79` and `06:75` say a deny is sticky. It's undefined what happens when, in a single evaluation, a matching `capture = "deny"` rule is followed by a matching `"allow"` rule. **Fix:** state that `capture` merges deny-first within one evaluation, or say exactly when a rule's deny becomes sticky.
3. **B01 row.** `13:31`'s B01 evidence column doesn't list XML provenance, signature or ancestry qualification, although `01:66` and `07:3` assign that work to B01. **Fix:** add "selected protocol XML provenance" to its evidence column.
4. **Wording:**
   - Every phase file's line 7 still says to see the roadmap for "effort estimates". It should say "sequencing".
   - `13:64` B17 says "accessible … UI by phase 5". It should say "semantic data model by phase 5; AT-SPI in phase 8".
   - `protocol-provenance.json:4`'s top-level `package_owner_check` still names only `xdg-shell`.
   - SEC-07 is tagged only `lock`, not `ime`. This has no effect on scheduling.

## 4. Verdict

- **Documentation:** internally consistent for the F1–F5 scope. The trust model, lock input routing, capture across lock, power-off lock coverage, phase gates, roadmap, window-state matrix and key-binding rules now agree with each other.
- **Phase 1:** ready to start.
- **Phase 2:** ready to start. The B16 design scope now includes security-context classification before privileged globals are registered.
- **Phase 3 and later:** no documentation blockers. Fix the low items whenever convenient.
- **Still unproven:** B04, B08, B15–B20, and every case remain `specified`. They need implementation evidence; that is the plan working as designed, not a contradiction in the documents.
