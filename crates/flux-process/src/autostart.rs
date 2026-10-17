//! Named/indexed, once-per-session direct-child autostart ownership.
use crate::{Error, ManagedChild, SpawnJob};
use std::os::unix::process::ExitStatusExt;
use std::{
    ffi::OsString,
    io,
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

pub const MAX_AUTOSTART_ENTRIES: usize = 128;
pub const MAX_AUTOSTART_IDENTITIES: usize = 256;
pub const MAX_AUTOSTART_PLAN_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_AUTOSTART_HISTORY_BYTES: usize = 8 * 1024 * 1024;
const GRACE: Duration = Duration::from_millis(250);

#[derive(Debug)]
pub struct AutostartCommand {
    pub id: Option<String>,
    pub argv: Vec<String>,
    pub restart_on_change: bool,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Identity {
    Index(usize),
    Named(String),
}
#[derive(Debug)]
struct Entry {
    identity: Identity,
    argv: Arc<Vec<String>>,
    bytes: usize,
    restart: bool,
}
/// Construct on a preparation worker. Keeps source-controlled strings out of
/// dispatch-time admission and status snapshots. Identity is an optional stable ID, or the legacy entry index.
#[derive(Debug)]
pub struct AutostartPlan {
    generation: u64,
    entries: Vec<Entry>,
    display: OsString,
    ipc: Option<PathBuf>,
}
impl AutostartPlan {
    pub fn prepare(
        generation: u64,
        commands: Vec<AutostartCommand>,
        display: OsString,
        ipc: Option<PathBuf>,
    ) -> Result<Arc<Self>, Error> {
        if generation == 0 || commands.len() > MAX_AUTOSTART_ENTRIES {
            return Err(Error::Invalid(
                "invalid autostart generation or entry count".into(),
            ));
        }
        crate::validate_environment_value(display.as_encoded_bytes(), "WAYLAND_DISPLAY")?;
        if let Some(path) = &ipc {
            crate::validate_environment_value(
                path.as_os_str().as_encoded_bytes(),
                "FLUXWM_SOCKET",
            )?;
        }
        let mut total = 0usize;
        let mut entries = Vec::with_capacity(commands.len());
        for (index, command) in commands.into_iter().enumerate() {
            let identity = match command.id {
                Some(id) if !id.is_empty() && id.len() <= 128 && !id.contains('\0') => {
                    Identity::Named(id)
                }
                Some(_) => return Err(Error::Invalid("invalid autostart identity".into())),
                None => Identity::Index(index),
            };
            if entries.iter().any(|e: &Entry| e.identity == identity) {
                return Err(Error::Invalid("duplicate autostart identity".into()));
            }
            flux_core::validate_argv(&command.argv)
                .map_err(|_| Error::Invalid("invalid autostart argv".into()))?;
            let bytes = command.argv.iter().map(String::len).sum::<usize>();
            total = total
                .checked_add(bytes)
                .ok_or_else(|| Error::Invalid("autostart byte accounting overflow".into()))?;
            if total > MAX_AUTOSTART_PLAN_BYTES {
                return Err(Error::Invalid(
                    "autostart plan exceeds 4MiB argv bytes".into(),
                ));
            }
            entries.push(Entry {
                identity,
                argv: Arc::new(command.argv),
                bytes,
                restart: command.restart_on_change,
            });
        }
        Ok(Arc::new(Self {
            generation,
            entries,
            display,
            ipc,
        }))
    }
    pub fn generation(&self) -> u64 {
        self.generation
    }
}
#[derive(Clone, Debug)]
pub struct AutostartStatus {
    pub slot: usize,
    pub id: Option<String>,
    pub index: Option<usize>,
    pub desired: bool,
    pub attempts: u64,
    pub starts: u64,
    pub pid: Option<u32>,
    pub state: &'static str,
    pub change_deferred: bool,
    pub error: Option<&'static str>,
    pub errno: Option<i32>,
    pub exit_code: Option<i32>,
    pub exit_signal: Option<i32>,
}
#[derive(Clone, Debug, Default)]
pub struct AutostartSnapshot {
    pub requested_generation: Option<u64>,
    pub active_generation: Option<u64>,
    pub active_complete: bool,
    pub retained_argv_bytes: usize,
    pub settled: bool,
    pub closed: bool,
    pub error: Option<&'static str>,
    pub entries: Vec<AutostartStatus>,
}
#[derive(Default)]
struct Control {
    latest: u64,
    pending: Option<Arc<AutostartPlan>>,
    discard: Option<Arc<AutostartPlan>>,
    reap: bool,
    closed: bool,
    snapshot: Option<AutostartSnapshot>,
}
#[derive(Default)]
struct Shared {
    control: Mutex<Control>,
    changed: Condvar,
}
pub struct AutostartWorker {
    shared: Arc<Shared>,
    thread: Option<JoinHandle<()>>,
}
impl AutostartWorker {
    pub fn new(notify: impl Fn() + Send + 'static) -> io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let owned = shared.clone();
        let thread = thread::Builder::new()
            .name("flux-autostart".into())
            .spawn(move || run(owned, notify))?;
        Ok(Self {
            shared,
            thread: Some(thread),
        })
    }
    /// Nonblocking latest-plan admission. QueueFull means retry the same pinned
    /// plan after a worker notification. No executable work or argv scans occur.
    pub fn try_install(&self, plan: &Arc<AutostartPlan>) -> Result<(), Error> {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if c.closed || self.thread.as_ref().is_none_or(|t| t.is_finished()) {
            return Err(Error::Stopped);
        }
        if plan.generation <= c.latest {
            return Err(Error::Invalid("autostart generation is stale".into()));
        }
        if c.pending.is_some() && c.discard.is_some() {
            return Err(Error::QueueFull);
        }
        c.latest = plan.generation;
        if let Some(old) = c.pending.replace(plan.clone()) {
            c.discard = Some(old);
        }
        self.shared.changed.notify_one();
        Ok(())
    }
    pub fn notify_child_exit(&self) {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        c.reap = true;
        self.shared.changed.notify_one();
    }
    pub fn take_snapshot(&self) -> Option<AutostartSnapshot> {
        self.shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .snapshot
            .take()
    }
    /// Seals admission without dropping plans, killing or waiting on dispatch.
    pub fn stop(&self) {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        c.closed = true;
        self.shared.changed.notify_one();
    }
    pub fn shutdown(&mut self) -> io::Result<()> {
        self.stop();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| io::Error::other("autostart worker panicked"))?;
        }
        Ok(())
    }
}
impl Drop for AutostartWorker {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
struct Slot {
    identity: Identity,
    history: Option<Arc<Vec<String>>>,
    bytes: usize,
    present: bool,
    restart_pending: bool,
    child: Option<ManagedChild>,
    view: AutostartStatus,
}
impl Slot {
    fn new(slot: usize, identity: Identity) -> Self {
        Self {
            identity: identity.clone(),
            history: None,
            bytes: 0,
            present: false,
            restart_pending: false,
            child: None,
            view: AutostartStatus {
                slot,
                id: match identity {
                    Identity::Named(id) => Some(id),
                    Identity::Index(_) => None,
                },
                index: None,
                desired: false,
                attempts: 0,
                starts: 0,
                pid: None,
                state: "never-started",
                change_deferred: false,
                error: None,
                errno: None,
                exit_code: None,
                exit_signal: None,
            },
        }
    }
    fn needs_start(&self, e: &Entry) -> bool {
        self.view.attempts == 0
            || (e.restart
                && (self.restart_pending
                    || !self.present
                    || self.history.as_deref() != Some(e.argv.as_ref())))
    }
    fn exited(&mut self, status: std::process::ExitStatus) {
        self.view.pid = None;
        self.view.state = "exited";
        self.view.exit_code = status.code();
        self.view.exit_signal = status.signal();
    }
}
fn current(shared: &Shared, generation: u64) -> bool {
    let c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
    !c.closed && c.latest == generation
}
#[derive(Default)]
struct Progress {
    active: Option<u64>,
    observed: Option<u64>,
    settled: bool,
    active_complete: bool,
    error: Option<&'static str>,
}
fn publish(shared: &Shared, slots: &[Slot], progress: &Progress, notify: &impl Fn()) {
    let mut snapshot = AutostartSnapshot {
        requested_generation: progress.observed,
        active_generation: progress.active,
        active_complete: progress.active_complete,
        retained_argv_bytes: slots.iter().map(|s| s.bytes).sum(),
        settled: progress.settled,
        closed: false,
        error: progress.error,
        entries: slots.iter().map(|s| s.view.clone()).collect(),
    };
    let old = {
        let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
        snapshot.closed = c.closed;
        c.snapshot.replace(snapshot)
    };
    drop(old);
    notify();
}
fn reap(slots: &mut [Slot]) -> bool {
    let mut changed = false;
    for slot in slots {
        if let Some(child) = slot.child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => {
                    slot.exited(status);
                    slot.child.take();
                    changed = true;
                }
                Ok(None) => (),
                Err(error) => {
                    if slot.view.error != Some("owned child reaping failed")
                        || slot.view.errno != error.raw_os_error()
                    {
                        changed = true;
                    }
                    slot.view.error = Some("owned child reaping failed");
                    slot.view.state = "ownership-error";
                    slot.view.errno = error.raw_os_error();
                }
            }
        }
    }
    changed
}
/// Broadcast TERM before a common grace interval, then retire remaining owned
/// children. No waitpid(-1), process-group signalling or descendant claim.
fn retire(slots: &mut [Slot], indices: &[usize]) {
    for &i in indices {
        if let Some(child) = slots[i].child.as_mut() {
            match child.try_wait() {
                Ok(Some(status)) => slots[i].exited(status),
                Ok(None) => {
                    // SAFETY: this exclusive unreaped Child keeps its PID from
                    // reuse. No external reaper may compete with this owner.
                    if unsafe { libc::kill(child.id() as libc::pid_t, libc::SIGTERM) } != 0 {
                        slots[i].view.error = Some("owned child termination failed");
                        slots[i].view.errno = io::Error::last_os_error().raw_os_error();
                    }
                }
                Err(error) => {
                    slots[i].view.error = Some("owned child reaping failed");
                    slots[i].view.errno = error.raw_os_error();
                }
            }
        }
    }
    let deadline = Instant::now() + GRACE;
    while Instant::now() < deadline {
        let mut live = false;
        for &i in indices {
            if let Some(child) = slots[i].child.as_mut() {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        slots[i].exited(status);
                        slots[i].child.take();
                    }
                    Ok(None) => live = true,
                    Err(_) => {
                        live = true;
                    }
                }
            }
        }
        if !live {
            break;
        }
        thread::sleep(Duration::from_millis(2));
    }
    for &i in indices {
        if let Some(mut child) = slots[i].child.take() {
            match child.try_wait() {
                Ok(Some(status)) => slots[i].exited(status),
                Ok(None) => {
                    let _ = child.child.kill();
                    match child.wait() {
                        Ok(status) => slots[i].exited(status),
                        Err(error) => {
                            slots[i].view.error = Some("owned child retirement failed");
                            slots[i].view.errno = error.raw_os_error();
                        }
                    }
                }
                Err(error) => {
                    slots[i].view.error = Some("owned child retirement failed");
                    slots[i].view.errno = error.raw_os_error();
                }
            }
        }
        slots[i].view.pid = None;
    }
}
fn run(shared: Arc<Shared>, notify: impl Fn()) {
    let mut slots: Vec<Slot> = Vec::new();
    let mut progress = Progress {
        settled: true,
        ..Progress::default()
    };
    loop {
        let (plan, discard, should_reap, closed) = {
            let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            while !c.closed && c.pending.is_none() && c.discard.is_none() && !c.reap {
                c = shared.changed.wait(c).unwrap_or_else(|e| e.into_inner());
            }
            let values = (c.pending.take(), c.discard.take(), c.reap, c.closed);
            c.reap = false;
            values
        };
        let discarded = discard.is_some();
        drop(discard);
        if discarded {
            notify();
        }
        if closed {
            break;
        }
        if should_reap && reap(&mut slots) {
            publish(&shared, &slots, &progress, &notify);
        }
        let Some(plan) = plan else {
            continue;
        };
        if !current(&shared, plan.generation) {
            continue;
        }
        progress.observed = Some(plan.generation);
        let added = plan
            .entries
            .iter()
            .filter(|entry| !slots.iter().any(|s| s.identity == entry.identity))
            .count();
        let mut bytes = 0usize;
        for slot in &slots {
            if let Some(entry) = plan.entries.iter().find(|e| e.identity == slot.identity) {
                bytes += if slot.needs_start(entry) {
                    // Any prefix can survive supersession. Account for both
                    // shrinking old histories and growing replacements.
                    slot.bytes.max(entry.bytes)
                } else {
                    slot.bytes
                };
            }
        }
        bytes += plan
            .entries
            .iter()
            .filter(|e| !slots.iter().any(|s| s.identity == e.identity))
            .map(|e| e.bytes)
            .sum::<usize>();
        if slots.len() + added > MAX_AUTOSTART_IDENTITIES || bytes > MAX_AUTOSTART_HISTORY_BYTES {
            progress.error = Some("autostart session identity or retained argv budget exceeded");
            progress.settled = true;
            publish(&shared, &slots, &progress, &notify);
            continue;
        }
        for entry in &plan.entries {
            if !slots.iter().any(|s| s.identity == entry.identity) {
                slots.push(Slot::new(slots.len(), entry.identity.clone()));
            }
        }
        let stop: Vec<_> = slots
            .iter()
            .enumerate()
            .filter_map(|(i, s)| {
                let desired = plan.entries.iter().find(|e| e.identity == s.identity);
                (s.child.is_some() && desired.is_none_or(|entry| s.needs_start(entry))).then_some(i)
            })
            .collect();
        progress.active = Some(plan.generation);
        progress.error = None;
        progress.settled = false;
        progress.active_complete = false;
        // A superseding plan can arrive during retirement. Preserve the need
        // to restart a deliberately retired child even if it restores argv.
        for &i in &stop {
            slots[i].restart_pending = true;
        }
        retire(&mut slots, &stop);
        for slot in &mut slots {
            if !plan.entries.iter().any(|e| e.identity == slot.identity) {
                slot.present = false;
                slot.restart_pending = false;
                slot.history = None;
                slot.bytes = 0;
                slot.view.desired = false;
                slot.view.index = None;
                slot.view.change_deferred = false;
                slot.view.state = "removed";
            }
        }
        publish(&shared, &slots, &progress, &notify);
        let mut complete = true;
        for (index, entry) in plan.entries.iter().enumerate() {
            if !current(&shared, plan.generation) {
                complete = false;
                break;
            }
            let slot = slots
                .iter_mut()
                .find(|s| s.identity == entry.identity)
                .expect("reserved identity");
            let start = slot.needs_start(entry);
            slot.present = true;
            slot.view.desired = true;
            slot.view.index = Some(index);
            slot.view.change_deferred = !start
                && slot
                    .history
                    .as_ref()
                    .is_some_and(|h| h.as_ref() != entry.argv.as_ref());
            if !start {
                if slot.history.is_none() {
                    slot.view.state = "once-suppressed";
                }
                continue;
            }
            let Some(attempts) = slot.view.attempts.checked_add(1) else {
                slot.view.error = Some("autostart attempt counter exhausted");
                continue;
            };
            slot.restart_pending = false;
            slot.history = Some(entry.argv.clone());
            slot.bytes = entry.bytes;
            slot.view.attempts = attempts;
            slot.view.error = None;
            slot.view.errno = None;
            slot.view.exit_code = None;
            slot.view.exit_signal = None;
            let job = SpawnJob {
                operation: plan.generation,
                argv: entry.argv.as_ref().clone(),
                wayland_display: plan.display.clone(),
                ipc_socket: plan.ipc.clone(),
            };
            match crate::launch(job) {
                Ok(child) => {
                    slot.view.starts = slot
                        .view
                        .starts
                        .checked_add(1)
                        .expect("starts bounded by attempts");
                    slot.view.pid = Some(child.id());
                    slot.view.state = "running";
                    slot.child = Some(child);
                }
                Err(error) => {
                    slot.view.state = "failed";
                    slot.view.pid = None;
                    slot.view.error = Some("autostart executable could not be started");
                    slot.view.errno = error.raw_os_error();
                }
            }
            reap(&mut slots);
            publish(&shared, &slots, &progress, &notify);
        }
        progress.settled = complete;
        progress.active_complete = complete;
        publish(&shared, &slots, &progress, &notify);
    }
    let indices: Vec<_> = (0..slots.len()).collect();
    retire(&mut slots, &indices);
    for s in &mut slots {
        s.view.state = "stopped";
        s.view.pid = None;
        s.history = None;
        s.bytes = 0;
    }
    progress.settled = true;
    publish(&shared, &slots, &progress, &notify);
    let (pending, discard) = {
        let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
        (c.pending.take(), c.discard.take())
    };
    drop((pending, discard));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::DirBuilderExt;
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "flux-auto-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::fs::DirBuilder::new()
                .mode(0o700)
                .create(&path)
                .unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    fn command(id: &str, arg: &str, restart: bool) -> AutostartCommand {
        AutostartCommand {
            id: Some(id.into()),
            argv: vec!["/bin/sleep".into(), arg.into()],
            restart_on_change: restart,
        }
    }
    fn plan(g: u64, commands: Vec<AutostartCommand>) -> Arc<AutostartPlan> {
        AutostartPlan::prepare(g, commands, "auto-test".into(), None).unwrap()
    }
    fn settled(worker: &AutostartWorker, g: u64) -> AutostartSnapshot {
        let until = Instant::now() + Duration::from_secs(5);
        loop {
            worker.notify_child_exit();
            if let Some(s) = worker.take_snapshot()
                && s.requested_generation == Some(g)
                && s.settled
            {
                return s;
            }
            assert!(
                Instant::now() < until,
                "autostart did not settle generation{g}"
            );
            thread::sleep(Duration::from_millis(2));
        }
    }
    fn reaped(pid: u32) {
        // WNOWAIT observes ownership without consuming an unrelated test child
        // if this PID has already been reused in the same test process.
        let mut info: libc::siginfo_t = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe {
                libc::waitid(
                    libc::P_PID,
                    pid as libc::id_t,
                    &mut info,
                    libc::WEXITED | libc::WNOHANG | libc::WNOWAIT,
                )
            },
            -1
        );
        assert_eq!(
            io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }

    #[test]
    fn once_restart_named_reorder_removal_and_reintroduction_own_real_children() {
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        worker
            .try_install(&plan(
                1,
                vec![command("a", "10", false), command("b", "10", true)],
            ))
            .unwrap();
        let first = settled(&worker, 1);
        let a = first.entries[0].pid.unwrap();
        let b = first.entries[1].pid.unwrap();
        worker
            .try_install(&plan(
                2,
                vec![command("b", "10", true), command("a", "11", false)],
            ))
            .unwrap();
        let same = settled(&worker, 2);
        assert_eq!(same.entries[0].pid, Some(a));
        assert!(same.entries[0].change_deferred);
        assert_eq!(same.entries[0].index, Some(1));
        assert_eq!(same.entries[1].pid, Some(b));
        assert_eq!(same.entries[1].index, Some(0));
        worker
            .try_install(&plan(3, vec![command("a", "11", true)]))
            .unwrap();
        let changed = settled(&worker, 3);
        let replacement = changed.entries[0].pid.unwrap();
        assert_ne!(replacement, a);
        assert_eq!(changed.entries[0].starts, 2);
        assert!(!changed.entries[0].change_deferred);
        reaped(a);
        reaped(b);
        worker.try_install(&plan(4, vec![])).unwrap();
        let removed = settled(&worker, 4);
        assert!(removed.entries.iter().all(|e| e.pid.is_none()));
        reaped(replacement);
        worker
            .try_install(&plan(5, vec![command("a", "11", false)]))
            .unwrap();
        let suppressed = settled(&worker, 5);
        assert!(suppressed.entries[0].pid.is_none());
        assert_eq!(suppressed.entries[0].attempts, 2);
        assert_eq!(suppressed.entries[0].state, "once-suppressed");
        assert!(!suppressed.entries[0].change_deferred);
        worker
            .try_install(&plan(6, vec![command("a", "11", true)]))
            .unwrap();
        let reintroduced = settled(&worker, 6);
        let last = reintroduced.entries[0].pid.unwrap();
        assert_eq!(reintroduced.entries[0].starts, 3);
        worker.shutdown().unwrap();
        reaped(last);
        let closed = worker.take_snapshot().unwrap();
        assert!(closed.closed && closed.settled);
        assert!(closed.entries.iter().all(|e| e.pid.is_none()));
    }
    #[test]
    fn fast_exit_and_failed_exec_are_not_repeated_on_unchanged_reload() {
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        let entries = || {
            vec![
                AutostartCommand {
                    id: Some("fast".into()),
                    argv: vec!["/bin/true".into()],
                    restart_on_change: true,
                },
                AutostartCommand {
                    id: Some("missing".into()),
                    argv: vec!["/missing/SECRET_AUTOSTART_ARGV".into()],
                    restart_on_change: true,
                },
            ]
        };
        worker.try_install(&plan(1, entries())).unwrap();
        let mut s = settled(&worker, 1);
        let until = Instant::now() + Duration::from_secs(3);
        while s.entries[0].state != "exited" {
            assert!(Instant::now() < until);
            worker.notify_child_exit();
            thread::sleep(Duration::from_millis(2));
            if let Some(new) = worker.take_snapshot() {
                s = new;
            }
        }
        assert_eq!(s.entries[0].starts, 1);
        assert_eq!(s.entries[0].exit_code, Some(0));
        assert_eq!(s.entries[1].starts, 0);
        assert_eq!(s.entries[1].state, "failed");
        assert!(!s.entries[1].error.unwrap().contains("SECRET"));
        worker.try_install(&plan(2, entries())).unwrap();
        let after = settled(&worker, 2);
        assert!(after.entries.iter().all(|e| e.attempts == 1));
        worker.shutdown().unwrap();
    }
    #[test]
    fn plan_validation_and_stale_admission_are_side_effect_free() {
        assert!(AutostartPlan::prepare(0, vec![], "x".into(), None).is_err());
        assert!(
            AutostartPlan::prepare(
                1,
                vec![command("dup", "10", false), command("dup", "11", false)],
                "x".into(),
                None
            )
            .is_err()
        );
        assert!(
            AutostartPlan::prepare(
                1,
                vec![AutostartCommand {
                    id: Some(String::new()),
                    argv: vec!["/bin/true".into()],
                    restart_on_change: false
                }],
                "x".into(),
                None
            )
            .is_err()
        );
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        let p = plan(2, vec![]);
        worker.try_install(&p).unwrap();
        settled(&worker, 2);
        assert!(worker.try_install(&p).is_err());
        assert!(worker.try_install(&plan(1, vec![])).is_err());
        worker.shutdown().unwrap();
        assert!(worker.try_install(&plan(3, vec![])).is_err());
    }
    #[test]
    fn session_identity_budget_rejects_whole_plan_without_starting_new_entries() {
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        for i in 0..MAX_AUTOSTART_IDENTITIES {
            worker
                .try_install(&plan(
                    i as u64 + 1,
                    vec![AutostartCommand {
                        id: Some(format!("entry-{i}")),
                        argv: vec!["/missing/flux-auto".into()],
                        restart_on_change: false,
                    }],
                ))
                .unwrap();
            let s = settled(&worker, i as u64 + 1);
            assert!(s.error.is_none());
        }
        worker
            .try_install(&plan(257, vec![command("excess", "10", false)]))
            .unwrap();
        let rejected = settled(&worker, 257);
        assert!(rejected.error.is_some());
        assert_eq!(rejected.active_generation, Some(256));
        assert_eq!(rejected.entries.len(), 256);
        assert!(rejected.entries.iter().all(|e| e.pid.is_none()));
        worker.shutdown().unwrap();
    }
    #[test]
    fn shutdown_uses_a_common_grace_for_term_ignoring_owned_children() {
        let temp = Temp::new();
        let script = temp.0.join("ignore.py");
        std::fs::write(&script,"import signal,time,sys\nsignal.signal(signal.SIGTERM,signal.SIG_IGN)\nopen(sys.argv[1],'w').write('ready')\ntime.sleep(10)\n").unwrap();
        let ready: Vec<_> = (0..6).map(|i| temp.0.join(format!("ready-{i}"))).collect();
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        worker
            .try_install(&plan(
                1,
                ready
                    .iter()
                    .enumerate()
                    .map(|(i, path)| AutostartCommand {
                        id: Some(format!("ignore-{i}")),
                        argv: vec![
                            "/usr/bin/python3".into(),
                            script.to_str().unwrap().into(),
                            path.to_str().unwrap().into(),
                        ],
                        restart_on_change: false,
                    })
                    .collect(),
            ))
            .unwrap();
        let pids: Vec<_> = settled(&worker, 1)
            .entries
            .iter()
            .map(|e| e.pid.unwrap())
            .collect();
        let until = Instant::now() + Duration::from_secs(3);
        while !ready.iter().all(|p| p.exists()) {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        let started = Instant::now();
        worker.shutdown().unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(1),
            "six children must share one grace interval"
        );
        for pid in pids {
            reaped(pid);
        }
        let s = worker.take_snapshot().unwrap();
        assert!(
            s.entries
                .iter()
                .all(|e| e.exit_signal == Some(libc::SIGKILL))
        );
    }
    #[test]
    fn superseded_retirement_restores_original_argv_for_restartable_identity() {
        let temp = Temp::new();
        let script = temp.0.join("term.py");
        let ready = temp.0.join("ready");
        let term = temp.0.join("term");
        std::fs::write(&script,"import signal,time,sys,os\nsignal.signal(signal.SIGTERM,lambda *_:open(sys.argv[2],'w').write('term'))\nopen(sys.argv[1],'w').write(str(os.getpid()))\ntime.sleep(10)\n").unwrap();
        let original = || AutostartCommand {
            id: Some("service".into()),
            argv: vec![
                "/usr/bin/python3".into(),
                script.to_str().unwrap().into(),
                ready.to_str().unwrap().into(),
                term.to_str().unwrap().into(),
            ],
            restart_on_change: true,
        };
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        worker.try_install(&plan(1, vec![original()])).unwrap();
        let pid = settled(&worker, 1).entries[0].pid.unwrap();
        let until = Instant::now() + Duration::from_secs(3);
        while !ready.exists() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        worker
            .try_install(&plan(2, vec![command("service", "10", true)]))
            .unwrap();
        while !term.exists() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        worker.try_install(&plan(3, vec![original()])).unwrap();
        worker.try_install(&plan(4, vec![original()])).unwrap();
        let retry = plan(5, vec![original()]);
        assert_eq!(worker.try_install(&retry), Err(Error::QueueFull));
        let latest = settled(&worker, 4);
        assert_ne!(latest.entries[0].pid, Some(pid));
        assert!(latest.entries[0].pid.is_some());
        assert_eq!(
            latest.entries[0].attempts, 2,
            "superseded command must not start"
        );
        reaped(pid);
        worker.try_install(&retry).unwrap();
        assert_eq!(settled(&worker, 5).entries[0].attempts, 2);
        let latest_pid = latest.entries[0].pid.unwrap();
        while std::fs::read_to_string(&ready).ok().as_deref() != Some(&latest_pid.to_string()) {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        std::fs::remove_file(&term).unwrap();
        worker
            .try_install(&plan(6, vec![command("service", "11", true)]))
            .unwrap();
        while !term.exists() {
            assert!(Instant::now() < until);
            thread::sleep(Duration::from_millis(2));
        }
        worker.try_install(&plan(7, vec![original()])).unwrap();
        worker.stop();
        worker.shutdown().unwrap();
        reaped(latest_pid);
        let closed = worker.take_snapshot().unwrap();
        assert!(closed.closed && closed.settled);
        assert_eq!(
            closed.entries[0].attempts, 2,
            "sealed pending plans must not execute"
        );
    }
    #[test]
    fn retained_argv_budget_rejects_before_effects_and_removal_releases_bytes() {
        let large = |id: String| AutostartCommand {
            id: Some(id),
            argv: std::iter::once("/missing/autostart".into())
                .chain((0..255).map(|_| "x".repeat(4096)))
                .collect(),
            restart_on_change: false,
        };
        let small = |id: String| AutostartCommand {
            id: Some(id),
            argv: vec!["/bin/true".into()],
            restart_on_change: false,
        };
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        worker
            .try_install(&plan(1, (0..4).map(|i| large(format!("a{i}"))).collect()))
            .unwrap();
        settled(&worker, 1);
        worker
            .try_install(&plan(
                2,
                (0..4)
                    .map(|i| small(format!("a{i}")))
                    .chain((0..4).map(|i| large(format!("b{i}"))))
                    .collect(),
            ))
            .unwrap();
        let accepted = settled(&worker, 2);
        assert_eq!(accepted.active_generation, Some(2));
        worker
            .try_install(&plan(
                3,
                (0..4)
                    .flat_map(|i| [small(format!("a{i}")), small(format!("b{i}"))])
                    .chain([large("excess".into())])
                    .collect(),
            ))
            .unwrap();
        let rejected = settled(&worker, 3);
        assert!(rejected.error.is_some());
        assert_eq!(rejected.active_generation, Some(2));
        assert_eq!(rejected.entries.len(), 8);
        assert!(rejected.entries.iter().all(|e| e.attempts == 1));
        worker.try_install(&plan(4, vec![])).unwrap();
        settled(&worker, 4);
        worker
            .try_install(&plan(5, vec![large("excess".into())]))
            .unwrap();
        assert_eq!(settled(&worker, 5).active_generation, Some(5));
        worker.shutdown().unwrap();
    }
    #[test]
    fn failed_replacement_retires_old_child_and_unchanged_failure_is_not_retried() {
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        worker
            .try_install(&plan(1, vec![command("service", "10", true)]))
            .unwrap();
        let pid = settled(&worker, 1).entries[0].pid.unwrap();
        let failed = || AutostartCommand {
            id: Some("service".into()),
            argv: vec!["/missing/autostart-replacement".into()],
            restart_on_change: true,
        };
        worker.try_install(&plan(2, vec![failed()])).unwrap();
        let replaced = settled(&worker, 2);
        reaped(pid);
        assert!(replaced.entries[0].pid.is_none());
        assert_eq!(replaced.entries[0].attempts, 2);
        assert_eq!(replaced.entries[0].starts, 1);
        assert_eq!(replaced.entries[0].state, "failed");
        worker.try_install(&plan(3, vec![failed()])).unwrap();
        assert_eq!(settled(&worker, 3).entries[0].attempts, 2);
        worker.shutdown().unwrap();
    }
    #[test]
    fn positional_zero_and_named_zero_have_separate_owned_children() {
        let mut worker = AutostartWorker::new(|| {}).unwrap();
        let mut positional = command("unused", "10", false);
        positional.id = None;
        worker
            .try_install(&plan(1, vec![positional, command("0", "10", false)]))
            .unwrap();
        let first = settled(&worker, 1);
        let indexed_pid = first.entries[0].pid.unwrap();
        let named_pid = first.entries[1].pid.unwrap();
        assert_ne!(indexed_pid, named_pid);
        assert!(first.entries[0].id.is_none());
        worker
            .try_install(&plan(2, vec![command("0", "10", false)]))
            .unwrap();
        let after = settled(&worker, 2);
        reaped(indexed_pid);
        assert_eq!(after.entries[0].state, "removed");
        assert_eq!(after.entries[1].pid, Some(named_pid));
        worker.shutdown().unwrap();
        reaped(named_pid);
    }
    // Pause after each real publication. A newer plan is admitted while the
    // worker owns a concrete prefix, without relying on scheduler timing.
    struct ControlledWorker {
        worker: AutostartWorker,
        notices: std::sync::mpsc::Receiver<()>,
        resume: std::sync::mpsc::Sender<()>,
        gated: Arc<std::sync::atomic::AtomicBool>,
    }
    impl ControlledWorker {
        fn new() -> Self {
            let (notice, notices) = std::sync::mpsc::channel();
            let (resume, resumed) = std::sync::mpsc::channel();
            let gated = Arc::new(std::sync::atomic::AtomicBool::new(true));
            let gate = gated.clone();
            let worker = AutostartWorker::new(move || {
                if gate.load(std::sync::atomic::Ordering::SeqCst) && notice.send(()).is_ok() {
                    let _ = resumed.recv_timeout(Duration::from_secs(3));
                }
            })
            .unwrap();
            Self {
                worker,
                notices,
                resume,
                gated,
            }
        }
        fn next(&self) -> AutostartSnapshot {
            let until = Instant::now() + Duration::from_secs(5);
            loop {
                match self.notices.recv_timeout(Duration::from_millis(2)) {
                    Ok(()) => return self.worker.take_snapshot().expect("published snapshot"),
                    Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                        assert!(Instant::now() < until, "controlled worker did not publish");
                        self.worker.notify_child_exit();
                    }
                    Err(error) => panic!("controlled worker disconnected: {error}"),
                }
            }
        }
        fn resume(&self) {
            self.resume.send(()).unwrap();
        }
        fn settled(&self, g: u64) -> AutostartSnapshot {
            loop {
                let s = self.next();
                self.resume();
                if s.requested_generation == Some(g) && s.settled {
                    return s;
                }
            }
        }
    }
    impl Drop for ControlledWorker {
        fn drop(&mut self) {
            self.gated.store(false, std::sync::atomic::Ordering::SeqCst);
            self.worker.stop();
            let _ = self.resume.send(());
            let _ = self.worker.shutdown();
        }
    }
    #[test]
    fn superseded_prefix_cannot_exceed_retained_history_budget() {
        let controlled = ControlledWorker::new();
        let large = |id: String| AutostartCommand {
            id: Some(id),
            argv: std::iter::once("/missing/autostart".into())
                .chain((0..255).map(|_| "x".repeat(4096)))
                .collect(),
            restart_on_change: false,
        };
        let small = |id: String, restart| AutostartCommand {
            id: Some(id),
            argv: vec!["/missing/small".into()],
            restart_on_change: restart,
        };
        controlled
            .worker
            .try_install(&plan(1, (0..4).map(|i| large(format!("a{i}"))).collect()))
            .unwrap();
        controlled.settled(1);
        controlled
            .worker
            .try_install(&plan(
                2,
                (0..4)
                    .map(|i| small(format!("a{i}"), false))
                    .chain((0..4).map(|i| large(format!("c{i}"))))
                    .collect(),
            ))
            .unwrap();
        let before = controlled.settled(2);
        assert!(before.retained_argv_bytes <= MAX_AUTOSTART_HISTORY_BYTES);
        let p3 = plan(
            3,
            (0..4)
                .map(|i| large(format!("b{i}")))
                .chain(
                    (0..4)
                        .flat_map(|i| [small(format!("a{i}"), true), small(format!("c{i}"), true)]),
                )
                .collect(),
        );
        let p4 = plan(4, vec![]);
        controlled.worker.try_install(&p3).unwrap();
        let mut peak = before.retained_argv_bytes;
        let rejected;
        loop {
            let s = controlled.next();
            peak = peak.max(s.retained_argv_bytes);
            let prefix = s
                .entries
                .iter()
                .any(|e| e.id.as_deref() == Some("b3") && e.attempts == 1);
            if s.settled || prefix {
                rejected = s.error.is_some();
                controlled.worker.try_install(&p4).unwrap();
                controlled.resume();
                break;
            }
            controlled.resume();
        }
        let removed = controlled.settled(4);
        assert_eq!(removed.retained_argv_bytes, 0);
        assert!(
            peak <= MAX_AUTOSTART_HISTORY_BYTES,
            "superseded prefix retained {peak} bytes"
        );
        assert!(
            rejected,
            "unsafe transition must be rejected before starting new identities"
        );
        assert_eq!(removed.entries.len(), before.entries.len());
    }
    #[test]
    fn rejected_latest_plan_keeps_partial_active_status_settled_after_child_exit() {
        let controlled = ControlledWorker::new();
        for (g, range) in [(1, 0..128), (2, 128..256)] {
            let entries = range
                .map(|i| AutostartCommand {
                    id: Some(format!("id-{i}")),
                    argv: vec!["/missing/history".into()],
                    restart_on_change: false,
                })
                .collect();
            controlled.worker.try_install(&plan(g, entries)).unwrap();
            controlled.settled(g);
        }
        controlled
            .worker
            .try_install(&plan(
                3,
                vec![command("id-0", "0.5", true), command("id-1", "10", true)],
            ))
            .unwrap();
        loop {
            let s = controlled.next();
            let started = s
                .entries
                .iter()
                .any(|e| e.id.as_deref() == Some("id-0") && e.starts == 1);
            if started {
                controlled
                    .worker
                    .try_install(&plan(4, vec![command("excess", "10", true)]))
                    .unwrap();
                controlled.resume();
                break;
            }
            controlled.resume();
        }
        let rejected = controlled.settled(4);
        assert!(rejected.error.is_some());
        assert_eq!(rejected.active_generation, Some(3));
        assert!(!rejected.active_complete);
        loop {
            let s = controlled.next();
            controlled.resume();
            assert!(s.settled, "reaping must not unset settled rejection status");
            assert!(!s.active_complete);
            assert_eq!(s.requested_generation, Some(4));
            assert!(s.error.is_some());
            if s.entries
                .iter()
                .find(|e| e.id.as_deref() == Some("id-0"))
                .unwrap()
                .pid
                .is_none()
            {
                break;
            }
        }
    }
    #[test]
    fn accepted_near_cap_prefixes_survive_three_restart_removal_readd_supersessions() {
        let controlled = ControlledWorker::new();
        let large = |id: &str| AutostartCommand {
            id: Some(id.into()),
            argv: std::iter::once("/missing/large".into())
                .chain((0..255).map(|_| "x".repeat(4096)))
                .collect(),
            restart_on_change: false,
        };
        let small = |id: &str, restart| AutostartCommand {
            id: Some(id.into()),
            argv: vec!["/missing/small".into()],
            restart_on_change: restart,
        };
        controlled
            .worker
            .try_install(&plan(1, (0..4).map(|i| large(&format!("a{i}"))).collect()))
            .unwrap();
        controlled.settled(1);
        controlled
            .worker
            .try_install(&plan(
                2,
                (0..4)
                    .map(|i| small(&format!("a{i}"), false))
                    .chain((0..3).map(|i| large(&format!("c{i}"))))
                    .collect(),
            ))
            .unwrap();
        let before = controlled.settled(2);
        let p3 = plan(
            3,
            std::iter::once(large("b0"))
                .chain((0..4).map(|i| small(&format!("a{i}"), true)))
                .chain((0..3).map(|i| small(&format!("c{i}"), true)))
                .collect(),
        );
        let p4 = plan(
            4,
            vec![
                small("d0", false),
                small("b0", false),
                small("a0", true),
                small("a1", true),
                small("c0", true),
            ],
        );
        let p5 = plan(5, vec![small("a2", false), small("b0", true), large("e0")]);
        let p6 = plan(6, vec![small("a2", false), small("b0", true), large("e0")]);
        controlled.worker.try_install(&p3).unwrap();
        let mut peak = before.retained_argv_bytes;
        for (generation, id, attempts, next) in
            [(3, "b0", 1, &p4), (4, "d0", 1, &p5), (5, "b0", 2, &p6)]
        {
            loop {
                let s = controlled.next();
                peak = peak.max(s.retained_argv_bytes);
                assert!(s.error.is_none());
                let prefix = s.requested_generation == Some(generation)
                    && s.entries
                        .iter()
                        .any(|e| e.id.as_deref() == Some(id) && e.attempts == attempts);
                if prefix {
                    assert!(!s.active_complete);
                    controlled.worker.try_install(next).unwrap();
                    controlled.resume();
                    break;
                }
                controlled.resume();
            }
        }
        let complete = controlled.settled(6);
        assert!(complete.active_complete && complete.error.is_none());
        assert!(peak <= MAX_AUTOSTART_HISTORY_BYTES);
        assert!(
            peak > MAX_AUTOSTART_HISTORY_BYTES - 1024 * 1024,
            "accepted prefix should exercise the near-cap boundary"
        );
        let a2 = complete
            .entries
            .iter()
            .find(|e| e.id.as_deref() == Some("a2"))
            .unwrap();
        assert_eq!(a2.state, "once-suppressed");
        assert_eq!(a2.attempts, 1);
        let e0 = complete
            .entries
            .iter()
            .find(|e| e.id.as_deref() == Some("e0"))
            .unwrap();
        assert_eq!(
            e0.attempts, 1,
            "reserved but never attempted identity must execute once"
        );
    }
}
