# Trusted application launch worker

`flux-process` moves application `Command::spawn` work out of compositor dispatch. It launches explicit argument vectors for trusted host applications and delivers process ownership through a calloop completion channel. It does not qualify session supervision, application restoration or a production desktop.

## Public API and integration

```rust,ignore
let (worker, completions) = flux_process::SpawnWorker::new()?;
// Register completions with the compositor's calloop event loop. Its callback
// records messages; Root owns every ManagedChild contained in those messages.
let job = flux_process::SpawnJob {
    operation,
    argv,
    wayland_display: socket_name.clone(),
    ipc_socket: Some(ipc_path.clone()),
};
worker.try_spawn(job)?; // nonblocking admission; exec has not necessarily happened
```

`SpawnJob` carries a Root-assigned `u64` operation ID, `Vec<String>` argv, actual `OsString` Wayland display name/path, and optional IPC socket path. `try_spawn` returns `Error::Invalid(String)`, `QueueFull`, or `Stopped`; a successful return means the job entered the worker queue. Root must perform current lock/permission and command admission checks, assign unique instance-scoped operation identities, and retain a bounded pending-operation ledger. The worker does not perform shell splitting, shell expansion or application discovery through app IDs/titles.

`SpawnOutcome { operation, result: Result<ManagedChild, String> }` reports an exec-start result. A successful exec is distinct from application readiness, window creation and successful long-term execution. Missing executables and other spawn errors produce a failed outcome with the same operation ID and do not stop subsequent jobs. The executable's later exit status belongs to its `ManagedChild` owner.

`ManagedChild` provides `id()`, nonblocking `try_wait()` and blocking `wait()`. Its destructor kills and waits for an unreaped live direct child. Root should try-wait immediately when receiving a successful outcome: SIGCHLD may have arrived before the completion. Root then stores running children in its bounded live-child collection and tries them again on SIGCHLD. Reaping must use those exclusive `ManagedChild` owners; a competing `waitpid(-1)` reaper or `SIGCHLD=SIG_IGN`/`SA_NOCLDWAIT` violates ownership. On unexpected `ECHILD`, cleanup avoids signaling a potentially reused PID rather than claiming successful reaping.

`launch(job)` is the synchronous entrypoint for startup or other non-dispatch contexts. It returns `ManagedChild` with the same validation and environment rules. Root may use it before entering the event loop when blocking startup work is permitted. Calling it from a Wayland/input/render/IPC callback would reintroduce synchronous exec work.

## Bounds and backpressure

There is one named worker thread and a synchronous input queue of eight jobs. Admission uses `try_send`; it never waits for input-queue space. The completion channel is separately bounded to eight outcomes. At most one launch or unsent outcome is active on the worker. This bounds worker-local queued jobs/results; Root must additionally cap **supervised children plus all pending accepted operations**, initially 64, before admission. It must count results still in the completion channel as pending until ownership is recorded. Process count cannot be inferred from the input queue length alone.

When completion delivery is full, the worker retains one outcome and retries at a five-millisecond interval with a shutdown-wakeable condition variable. It does not launch further jobs while completion delivery is backpressured. This bounds children waiting for Root and permits shutdown without blocking forever in a completion-channel send. Root should service completion notifications promptly; the retry interval is a scheduling policy, not a measured latency target.

Delivery and worker shutdown are serialized under one control mutex. A result sent before the stop boundary is receiver-owned, including a result queued but not yet processed. An active or unsent result after that boundary is dropped on the worker, so its `ManagedChild` is killed/reaped. On receiver disconnection, an undeliverable child is killed/reaped and queued jobs are discarded without launching. Operation cancellations caused by shutdown are resolved by Root's pending-operation ledger; the worker does not emit one cancellation outcome per discarded job.

Root must not drop an unexpected successful outcome inside an event callback: `ManagedChild::drop` may wait. Preserve ownership for cleanup after dispatch or on the shutdown path. Admission bounds must prevent a successful completion from overflowing the live-child collection. There is no per-job cancellation interface yet; define whether previously accepted launches continue when a lock transition occurs rather than assuming queued jobs recheck compositor authority.

## Environment, descriptors and signals

Both admission and `launch` call shared `flux_core::validate_argv`: 1..256 arguments, each at most 4096 UTF-8 bytes, nonempty executable, no embedded NUL; later empty arguments are preserved. Wayland/IPC environment values accept 1..4096 NUL-free OS bytes. No filesystem existence check or PATH search occurs on the compositor admission path; the worker's `Command::spawn` handles executable resolution.

Every child gets the exact `WAYLAND_DISPLAY`. `WAYLAND_SOCKET` and `DISPLAY` are removed. `FLUXWM_SOCKET` is set to the provided instance path or explicitly removed if absent, preventing inheritance of another instance's IPC path. Other host environment, working directory, process group and standard IO retain `Command` defaults. In particular, this trusted-host application launcher preserves host session environment and does not provide the isolated configuration-helper environment required by [helper containment](../21-review-resolutions.md).

The compositor must block SIGINT, SIGTERM and SIGCHLD and establish its signal event source before creating worker/backend threads. The worker inherits that mask. An async-signal-safe `pre_exec` hook unblocks **only SIGINT, SIGTERM and SIGCHLD in the child**; every other inherited mask remains unchanged. The hook uses `sigemptyset`, `sigaddset`, `sigprocmask` and an errno-based error return, without locks, allocation, environment access or logging. It does not change the parent's mask. [Rust's pre_exec contract](https://doc.rust-lang.org/std/os/unix/process/trait.CommandExt.html#tymethod.pre_exec) describes post-fork restrictions; Linux documents [async-signal-safe operations](https://man7.org/linux/man-pages/man7/signal-safety.7.html) and [mask inheritance across fork/exec](https://man7.org/linux/man-pages/man7/signal.7.html).

All compositor-owned DRM/input/display/IPC/worker descriptors must remain close-on-exec; Root must audit native-library descriptors and explicit inheritance separately. This crate does not close every nonstandard inherited descriptor, create a process group/cgroup, or supervise descendants. Killing a direct child does not prove that grandchildren or daemonized application processes have terminated. A complete session/application ownership policy remains a later integration requirement.

## Shutdown and failure scope

Call `shutdown(self)` after leaving compositor event dispatch. It stops admission, discards queued jobs, wakes completion backpressure, joins the worker, and reports a worker-thread panic as an IO error. `Drop` performs the same stop/join as best-effort cleanup. Root must then drain or drop its completion channel and owned children outside dispatch; outcomes already delivered before shutdown remain Root's responsibility. Dropping the worker alone deliberately does not terminate a successfully delivered Root-owned child.

`Command::spawn`, child kill/wait and worker join can wait for kernel/filesystem/exec work. This crate does not impose a universal wall-clock exec or termination deadline, and stopping cannot interrupt a kernel-blocked active launch. The off-thread boundary preserves compositor dispatch while it runs, but final shutdown may still wait. SIGKILL does not guarantee immediate retirement of a process stuck in uninterruptible kernel work. Stronger shutdown containment needs a separate process/session policy and qualified failure behavior.

calloop 0.14.5's channel constructor can panic when its notifier cannot be created. `SpawnWorker::new` catches that unwind and returns an IO error, without changing the global panic hook; an existing panic hook can still print the original diagnostic. A `panic=abort` build cannot recover such a dependency panic and must document that build policy. The worker-thread creation itself uses a fallible thread builder. Allocation failure and catastrophic kernel/resource failure are not universally recoverable here.

## Verification

```sh
cargo test -p flux-process
cargo clippy -p flux-process --all-targets -- -D warnings
```

Tests launch this crate's own Rust test executable as a disposable fixture. They exercise actual calloop completion delivery; exact literal arguments and environment overrides; a child inheriting blocked SIGINT/SIGTERM/SIGCHLD/SIGUSR1 and observing only the first three unblocked; shared argv/environment rejection; missing executable delivery followed by a successful job; live-child kill/reap and cached exit status; deterministic eight-job queue saturation; shutdown while an active launch is gated; receiver loss; and shutdown with a full bounded completion channel. Every disposable child PID is checked as reaped after its owner cleanup. Tests do not mutate the parent process environment and only alter the executing test thread's signal mask, restoring it afterward.

These cases prove the local worker ownership and notification behavior in the test environment. They do not establish compositor-wide admission/reaping correctness, native session process ownership, descendant cleanup, executable readiness, or hard shutdown deadlines. Integration must additionally verify signal-source creation order, fast exits before completion, bounded diagnostics/history, live/pending count enforcement, and shutdown after a real IPC spawn request while input/render processing continues.
