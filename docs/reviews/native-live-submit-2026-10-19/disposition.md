# Native live submission review disposition

Claude Opus 5.5 inspected 31 frozen worktree inputs in 20 Read/Grep calls. [Verified execution](verified-execution.json) records a successful result, unchanged input hashes, no denied tools and no paths outside the snapshot. The [report](claude-report.md) found two medium issues and one low issue; this input is identified by its manifest, not by its base commit alone.

- **M1, initial event request:** the first modeset is now blocking ALLOW_MODESET without PAGE_FLIP_EVENT/NONBLOCK. A successful return authorizes exact-cookie `complete_blocking`; all syscall errors retain uncertain resources. Later FB-only flips retain cookie-bearing events. The probe counts one synchronous completion and two matching flips.
- **M2, unsupported CLOSEFB:** snapshot capture now creates and CLOSEFB-closes an unattached temporary framebuffer before keeping each rollback clone. Failure removes the unattached probe and aborts before any live modeset. Unexpected later CLOSEFB failure can still blank restored scanout at process exit; retention only lasts while the process lives. That durable recovery limit remains explicit.
- **LOW, failed GETFB2:** handle ownership starts only after a successful ioctl, avoiding closure of numbers the kernel may already have reclaimed.

[Focused follow-up](../native-live-submit-followup-2026-10-19/disposition.md) checks those corrections. The [implementation contract](../../development/native-live-submit.md) and [author validation](../../evidence/nested-milestone/2026-10-19-native-live-submit/README.md) retain hardware, rollback, recovery and compositor integration gaps. No physical session or full production gate is qualified.
