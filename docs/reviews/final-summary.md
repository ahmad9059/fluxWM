# Independent review outcome

Three actual Claude Code review passes completed successfully. The raw outputs preserve model, duration, tool-limit and permission metadata. The primary review model was `claude-opus-5-5`. Reviewer command/write tools were unavailable; no subagents were spawned.

1. [Initial review](claude-review.md): 15 findings, including invalid phase gates and inconsistent trust/capture contracts. [Disposition](disposition.md).
2. [Follow-up](claude-followup.md): five remaining high/medium groups and low items. [Disposition](followup-disposition.md).
3. [Final focused review](claude-final.md): F1–F5 resolved; no new/remaining high or medium contradictions in its stated scope; ready to begin phased implementation at phase 1.

The final reviewer did not certify runtime/production behavior, recompute source hashes, run tests or re-fetch sources. Its file-by-file scope and limitations remain in the unedited report. The author checked all [67 frozen final-review inputs](../evidence/final-review-snapshot.json) immediately after completion: [hash result](final-frozen-input-check.json). Earlier frozen follow-up inputs were checked similarly.

## Low items addressed after the final review

The final report's low suggestions were applied: exact helper spawn section now includes explicit fd closure; capture rules use deny-first merging within one evaluation; B01 explicitly owns selected XML provenance; phase text no longer refers to withdrawn estimates; B17 distinguishes phase-5 semantic groundwork from phase-8 AT-SPI qualification; every XML path has its own package owner record; SEC-07 includes IME dependency. GPU-07 full qualification now explicitly tests timeline release in phase 9, while its phase-3 implicit variant tests ownership retention without duplicating the full case.

These final small corrections were author-verified with the documentation checker after review; they were not separately re-reviewed by Claude. [Post-review changed-file hashes](post-review-changes.json) distinguish them from the frozen reviewed inputs. Administrative index/report additions are also identified rather than claimed byte-identical.

## Current evidence strength

- 94 full cases and 11 narrow early variants are specified; none is a passed FluxWM runtime test.
- Source evidence includes 38 retrieved files at six repository commits, file hashes, and a 13-file distro comparison with one explicit DMA-BUF difference.
- Explicit Lua 5.4 syntax/evaluation and fixture contract checks pass. Eight malformed fixtures were rejected as expected. A separately pinned Rust/mlua probe reproduced catchable hook errors and changed the config containment design.
- Local links, phase structure, declared feature/qualification order, case table agreement and evidence integrity pass the checker.

Implementation blockers B01–B21 retain their required evidence, owner role and deadlines. Driver support, lock/suspend safety, sandbox integration, font/assistive stack, backend fences and complete desktop readiness require actual implementation and qualification.
