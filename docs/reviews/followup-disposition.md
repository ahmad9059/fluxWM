# Follow-up finding disposition

The [unchanged follow-up report](claude-followup.md) and [raw metadata](claude-followup-result.json) record a successful second Claude review. The author recomputed all 60 frozen input hashes immediately after review completion: [result](frozen-input-check.json). Changes below were made **after** that checkpoint, so that snapshot describes the reviewed remediation, not these later edits.

| Finding | Correction | Proof still required |
| --- | --- | --- |
| F1 security-context scheduling | Protocol row split into phase-2 design / phase-7 implementation; phase-7 explicit task; security_context feature and SEC-04/05/06/11 tags; untagged/old sandbox classified host limitation stated | Actual broker listener/filter/nesting tests |
| F2 remaining early gates | direct_scanout feature phase 9; GPU-05:primary-plane phase 3; A11Y-02 full search phase 8 plus text fixture phase 5; B17 tasks in phases 3/5; SEC-12 full lock phase 7 with host-control variant phase 6 | Runtime evidence per narrowed/full cases |
| F3 host lock input authority | Chose option (a): any authenticated trusted-host IME/virtual keyboard can type only into current lock tree; explicitly no stronger host-program isolation; tagged sandbox injection and compositor-wide lock key observation denied | Current lock focus/seat/IME/OSK tests; normal application receives no lock input |
| F4 power-off locking | Chose backend-verified disabled/plane-cleared coverage with no lock-caused wake; first later frame black/lock; qualified active-black fallback only if needed, otherwise no successful lock | B08 exact protocol/KMS/power/suspend proof; no driver guarantee asserted |
| F5 prose/task contradictions | Lock-redaction replaces revocation wording; new locked capture grants denied (CAP-12); roadmap table regenerated with real dependencies and no residual forecast; W11 in phase 9; coroutine absent test; old power/font choice text points to selected contract | Phase gates and actual complete session evidence |

Low items were also addressed: immutable XML links and provenance caveat, package ownership recorded for all 13 XML files, phase-7 strip-native state variant, minimize begins phase 4, maximized validation tied to acknowledged configure/new commit, canonical `space` keysym and Shift rule, sticky privacy deny precedence, measurable stack/keymap oracles, precise clipboard/X11/IME tags, and explicit fd-close allowlist before helper exec.

All 94 full cases and 11 early variants remain `specified`. Fixture checks and source observations do not promote them to runtime passes. A final focused Claude consistency check covers these corrections; its narrower scope and limitations are recorded in its own report.
