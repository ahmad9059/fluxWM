//! Trusted-host application launching. Never evaluate Lua or split shell strings here.
//! Worker and ManagedChild shutdown can wait for the OS; drop outside event dispatch.
mod autostart;
pub use autostart::*;

use calloop::channel::{Channel, SyncSender as CompletionSender};
use std::{
    ffi::OsString,
    fmt, io,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::PathBuf,
    process::{Child, Command, ExitStatus},
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

pub const QUEUE_CAPACITY: usize = 8;
pub const COMPLETION_CAPACITY: usize = 8;
const RETRY_INTERVAL: Duration = Duration::from_millis(5);

#[derive(Debug, Clone)]
pub struct SpawnJob {
    pub operation: u64,
    pub argv: Vec<String>,
    pub wayland_display: OsString,
    pub ipc_socket: Option<PathBuf>,
}
impl SpawnJob {
    pub fn validate(&self) -> Result<(), Error> {
        flux_core::validate_argv(&self.argv).map_err(|e| Error::Invalid(e.to_string()))?;
        validate_environment_value(self.wayland_display.as_bytes(), "WAYLAND_DISPLAY")?;
        if let Some(path) = &self.ipc_socket {
            validate_environment_value(path.as_os_str().as_bytes(), "FLUXWM_SOCKET")?;
        }
        Ok(())
    }
}
fn validate_environment_value(value: &[u8], name: &str) -> Result<(), Error> {
    if value.is_empty() || value.len() > 4096 || value.contains(&0) {
        return Err(Error::Invalid(format!(
            "{name}: expected 1..4096 NUL-free bytes"
        )));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Error {
    Invalid(String),
    QueueFull,
    Stopped,
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invalid(message) => f.write_str(message),
            Self::QueueFull => f.write_str("application spawn queue is full"),
            Self::Stopped => f.write_str("application spawn worker is stopped"),
        }
    }
}
impl std::error::Error for Error {}

/// Owns one direct child. No external waitpid reaper or SIGCHLD auto-reap may
/// compete with this owner. Descendants/process groups require separate policy.
#[derive(Debug)]
pub struct ManagedChild {
    child: Child,
}
impl ManagedChild {
    pub fn id(&self) -> u32 {
        self.child.id()
    }
    pub fn try_wait(&mut self) -> io::Result<Option<ExitStatus>> {
        self.child.try_wait()
    }
    /// Blocking; call outside compositor event dispatch.
    pub fn wait(&mut self) -> io::Result<ExitStatus> {
        self.child.wait()
    }
}
impl Drop for ManagedChild {
    fn drop(&mut self) {
        match self.child.try_wait() {
            Ok(Some(_)) => return,
            // If ownership was violated by an external reaper, do not signal a
            // potentially reused PID. This is not successful owned-child cleanup.
            Err(error) if error.raw_os_error() == Some(libc::ECHILD) => return,
            _ => {}
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[derive(Debug)]
pub struct SpawnOutcome {
    pub operation: u64,
    pub result: Result<ManagedChild, String>,
}

fn command(job: &SpawnJob) -> Command {
    let mut command = Command::new(&job.argv[0]);
    command.args(&job.argv[1..]);
    command.env("WAYLAND_DISPLAY", &job.wayland_display);
    command.env_remove("WAYLAND_SOCKET");
    command.env_remove("DISPLAY");
    match &job.ipc_socket {
        Some(path) => {
            command.env("FLUXWM_SOCKET", path);
        }
        None => {
            command.env_remove("FLUXWM_SOCKET");
        }
    }
    // Root blocks termination/child-notification signals before backend/worker
    // threads. Exec inherits the thread mask unless explicitly changed.
    // Only async-signal-safe functions run in this post-fork child hook.
    unsafe {
        command.pre_exec(|| {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            if libc::sigemptyset(&mut signals) != 0
                || libc::sigaddset(&mut signals, libc::SIGINT) != 0
                || libc::sigaddset(&mut signals, libc::SIGTERM) != 0
                || libc::sigaddset(&mut signals, libc::SIGCHLD) != 0
                || libc::sigprocmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut()) != 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }
    command
}

/// Synchronous startup/worker launcher. Command::spawn may wait for filesystem,
/// exec or kernel work; never call it from compositor event dispatch.
pub fn launch(job: SpawnJob) -> io::Result<ManagedChild> {
    job.validate()
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
    command(&job).spawn().map(|child| ManagedChild { child })
}

struct Control {
    stopping: AtomicBool,
    mutex: Mutex<()>,
    changed: Condvar,
}
impl Control {
    fn stopped(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }
    fn stop(&self) {
        let _guard = self.mutex.lock().unwrap_or_else(|error| error.into_inner());
        self.stopping.store(true, Ordering::Release);
        self.changed.notify_all();
    }
    fn pause(&self) {
        let guard = self.mutex.lock().unwrap_or_else(|e| e.into_inner());
        if !self.stopped() {
            let _ = self.changed.wait_timeout(guard, RETRY_INTERVAL);
        }
    }
}

/// One worker, eight queued jobs, eight queued outcomes and at most one active
/// launch/unsent outcome. Root must also cap children plus pending operations.
pub struct SpawnWorker {
    sender: Option<SyncSender<SpawnJob>>,
    control: Arc<Control>,
    thread: Option<JoinHandle<()>>,
}
impl SpawnWorker {
    pub fn new() -> io::Result<(Self, Channel<SpawnOutcome>)> {
        Self::with_launcher(launch)
    }

    fn with_launcher(
        launcher: impl FnMut(SpawnJob) -> io::Result<ManagedChild> + Send + 'static,
    ) -> io::Result<(Self, Channel<SpawnOutcome>)> {
        // calloop 0.14.5 exposes only an infallible channel constructor and panics
        // when its notifier cannot be created. Translate that resource failure
        // without mutating the process-global panic hook.
        let (completion_sender, completion_receiver) = std::panic::catch_unwind(|| {
            calloop::channel::sync_channel::<SpawnOutcome>(COMPLETION_CAPACITY)
        })
        .map_err(|_| io::Error::other("could not create application completion channel"))?;
        let (sender, receiver) = mpsc::sync_channel(QUEUE_CAPACITY);
        let control = Arc::new(Control {
            stopping: AtomicBool::new(false),
            mutex: Mutex::new(()),
            changed: Condvar::new(),
        });
        let thread_control = control.clone();
        let thread = thread::Builder::new()
            .name("flux-spawn".into())
            .spawn(move || run(receiver, completion_sender, thread_control, launcher))?;
        Ok((
            Self {
                sender: Some(sender),
                control,
                thread: Some(thread),
            },
            completion_receiver,
        ))
    }

    /// Nonblocking admission only; acceptance does not mean exec completed.
    /// Job identity uniqueness and compositor lock/permission checks belong to Root.
    pub fn try_spawn(&self, job: SpawnJob) -> Result<(), Error> {
        if self.control.stopped() {
            return Err(Error::Stopped);
        }
        job.validate()?;
        match self.sender.as_ref().ok_or(Error::Stopped)?.try_send(job) {
            Ok(()) => Ok(()),
            Err(TrySendError::Full(_)) => Err(Error::QueueFull),
            Err(TrySendError::Disconnected(_)) => {
                self.control.stop();
                Err(Error::Stopped)
            }
        }
    }
    pub fn is_stopped(&self) -> bool {
        self.control.stopped()
    }
    /// Explicit blocking shutdown for reporting worker failure. Already delivered
    /// outcomes/children belong to the receiver and are not reclaimed by this call.
    pub fn shutdown(mut self) -> io::Result<()> {
        self.stop_and_join()
    }
    fn stop_and_join(&mut self) -> io::Result<()> {
        self.control.stop();
        self.sender.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| io::Error::other("application spawn worker panicked"))?;
        }
        Ok(())
    }
}
impl Drop for SpawnWorker {
    fn drop(&mut self) {
        let _ = self.stop_and_join();
    }
}
fn run(
    receiver: Receiver<SpawnJob>,
    completion: CompletionSender<SpawnOutcome>,
    control: Arc<Control>,
    mut launcher: impl FnMut(SpawnJob) -> io::Result<ManagedChild>,
) {
    'worker: while !control.stopped() {
        let job = match receiver.recv() {
            Ok(job) => job,
            Err(_) => break,
        };
        if control.stopped() {
            break;
        }
        let operation = job.operation;
        let mut outcome = SpawnOutcome {
            operation,
            result: launcher(job).map_err(|error| error.to_string()),
        };
        loop {
            // Delivery and stop are linearized: a result sent before stop is
            // receiver-owned; an active result after stop is killed/reaped here.
            let delivery = {
                let guard = control
                    .mutex
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                if control.stopped() {
                    drop(guard);
                    drop(outcome);
                    break 'worker;
                }
                completion.try_send(outcome)
            };
            match delivery {
                Ok(()) => break,
                Err(TrySendError::Full(pending)) => {
                    outcome = pending;
                    control.pause();
                }
                Err(TrySendError::Disconnected(undelivered)) => {
                    drop(undelivered);
                    control.stop();
                    break 'worker;
                }
            }
        }
    }
    // Dropping receiver discards queued SpawnJob data without executing it.
}

#[cfg(test)]
mod tests {
    use super::*;
    use calloop::{EventLoop, channel::Event};
    use std::{fs, os::unix::fs::DirBuilderExt, time::Instant};

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let path = std::env::temp_dir().join(format!(
                "flux-process-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn job(operation: u64, mode: &str) -> SpawnJob {
        SpawnJob {
            operation,
            argv: vec![
                std::env::current_exe().unwrap().to_str().unwrap().into(),
                "--exact".into(),
                "tests::child_fixture_entry".into(),
                "--nocapture".into(),
                "--test-threads=1".into(),
            ],
            wayland_display: format!("flux-process-fixture-{mode}").into(),
            ipc_socket: None,
        }
    }
    fn until(mut predicate: impl FnMut() -> bool) {
        let deadline = Instant::now() + Duration::from_secs(3);
        while !predicate() {
            assert!(Instant::now() < deadline, "condition timed out");
            thread::sleep(Duration::from_millis(1));
        }
    }
    fn assert_reaped(pid: u32) {
        let mut status = 0;
        let result = unsafe { libc::waitpid(pid as libc::pid_t, &mut status, libc::WNOHANG) };
        let error = io::Error::last_os_error();
        // Failed assertions must not leave the disposable fixture running.
        if result == 0 {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
                libc::waitpid(pid as libc::pid_t, &mut status, 0);
            }
        }
        assert_eq!(result, -1, "child {pid} was not reaped by its owner");
        assert_eq!(error.raw_os_error(), Some(libc::ECHILD));
    }
    struct Mask(libc::sigset_t);
    impl Mask {
        fn block() -> Self {
            unsafe {
                let mut set: libc::sigset_t = std::mem::zeroed();
                let mut old = std::mem::zeroed();
                libc::sigemptyset(&mut set);
                for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGCHLD, libc::SIGUSR1] {
                    libc::sigaddset(&mut set, signal);
                }
                assert_eq!(libc::pthread_sigmask(libc::SIG_BLOCK, &set, &mut old), 0);
                Self(old)
            }
        }
    }
    impl Drop for Mask {
        fn drop(&mut self) {
            assert_eq!(
                unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, &self.0, std::ptr::null_mut()) },
                0
            );
        }
    }

    /// Invoked as a separate executable process by this crate's process tests.
    /// Ordinary test runs return immediately; no process-global env mutation occurs.
    #[test]
    fn child_fixture_entry() {
        let Some(mode) = std::env::var_os("WAYLAND_DISPLAY") else {
            return;
        };
        if mode == "flux-process-fixture-sleep" {
            thread::sleep(Duration::from_secs(60));
            return;
        }
        if mode != "flux-process-fixture-report" {
            return;
        }
        let path = std::env::var_os("FLUXWM_SOCKET").expect("fixture report path");
        let mut mask: libc::sigset_t = unsafe { std::mem::zeroed() };
        assert_eq!(
            unsafe { libc::pthread_sigmask(libc::SIG_SETMASK, std::ptr::null(), &mut mask) },
            0
        );
        let mut report = format!(
            "wayland={mode:?}\nipc={path:?}\ndisplay={:?}\nwayland_socket={:?}\n",
            std::env::var_os("DISPLAY"),
            std::env::var_os("WAYLAND_SOCKET")
        );
        for (name, signal) in [
            ("int", libc::SIGINT),
            ("term", libc::SIGTERM),
            ("chld", libc::SIGCHLD),
            ("usr1", libc::SIGUSR1),
        ] {
            report.push_str(&format!("{name}={}\n", unsafe {
                libc::sigismember(&mask, signal)
            }));
        }
        report.push_str(&format!(
            "args={:?}\n",
            std::env::args().collect::<Vec<_>>()
        ));
        fs::write(path, report).unwrap();
    }

    #[test]
    fn validates_argv_environment_and_missing_executable() {
        let mut missing = job(1, "exit");
        missing.argv[0] = format!("/flux-process-no-executable-{}", std::process::id());
        assert_eq!(launch(missing).unwrap_err().kind(), io::ErrorKind::NotFound);
        let (worker, receiver) = SpawnWorker::new().unwrap();
        for argv in [
            vec![],
            vec!["".into()],
            vec!["bad\0name".into()],
            vec!["x".repeat(4097)],
        ] {
            let mut invalid = job(1, "exit");
            invalid.argv = argv;
            assert!(matches!(worker.try_spawn(invalid), Err(Error::Invalid(_))));
        }
        for display in [
            OsString::new(),
            OsString::from("bad\0display"),
            OsString::from("x".repeat(4097)),
        ] {
            let mut invalid = job(1, "exit");
            invalid.wayland_display = display;
            assert!(invalid.validate().is_err());
        }
        for path in [PathBuf::new(), PathBuf::from("bad\0socket")] {
            let mut invalid = job(1, "exit");
            invalid.ipc_socket = Some(path);
            assert!(invalid.validate().is_err());
        }
        worker.shutdown().unwrap();
        drop(receiver);
    }

    #[test]
    fn exact_arguments_and_environment_overrides_do_not_split_or_inherit_display_fd() {
        let mut request = job(1, "exit");
        request.argv = vec![
            "literal-program".into(),
            "".into(),
            "one argument $(not-a-shell) ; 'quotes'".into(),
        ];
        let command = command(&request);
        assert_eq!(command.get_program(), "literal-program");
        assert_eq!(
            command
                .get_args()
                .map(|s| s.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["", "one argument $(not-a-shell) ; 'quotes'"]
        );
        let env = command.get_envs().collect::<Vec<_>>();
        for removed in ["DISPLAY", "WAYLAND_SOCKET", "FLUXWM_SOCKET"] {
            assert!(
                env.iter()
                    .any(|(key, value)| *key == removed && value.is_none())
            );
        }
        assert!(env.iter().any(|(key, value)| *key == "WAYLAND_DISPLAY"
            && *value == Some(request.wayland_display.as_os_str())));
    }

    #[test]
    fn actual_child_gets_instance_environment_and_only_three_masks_unblocked() {
        let temp = Temp::new();
        let _mask = Mask::block();
        let (worker, receiver) = SpawnWorker::new().unwrap();
        let mut request = job(73, "report");
        let report = temp.0.join("report");
        request.ipc_socket = Some(report.clone());
        request
            .argv
            .extend(["--skip".into(), "literal $(not-a-shell) ; spaces".into()]);
        worker.try_spawn(request).unwrap();
        let mut completed = None;
        until(|| match receiver.try_recv() {
            Ok(outcome) => {
                completed = Some(outcome);
                true
            }
            Err(mpsc::TryRecvError::Empty) => false,
            Err(error) => panic!("{error}"),
        });
        let outcome = completed.unwrap();
        assert_eq!(outcome.operation, 73);
        let mut child = outcome.result.unwrap();
        let pid = child.id();
        assert!(child.wait().unwrap().success());
        drop(child);
        assert_reaped(pid);
        let text = fs::read_to_string(report).unwrap();
        for expected in [
            "wayland=\"flux-process-fixture-report\"",
            "display=None",
            "wayland_socket=None",
            "int=0",
            "term=0",
            "chld=0",
            "usr1=1",
            "literal $(not-a-shell) ; spaces",
        ] {
            assert!(text.contains(expected), "missing {expected}: {text}");
        }
        worker.shutdown().unwrap();
        drop(receiver);
    }

    #[test]
    fn managed_child_drop_kills_and_reaps_and_wait_caches_exit() {
        let child = launch(job(1, "sleep")).unwrap();
        let pid = child.id();
        drop(child);
        assert_reaped(pid);
        let mut child = launch(job(2, "exit")).unwrap();
        let pid = child.id();
        let status = child.wait().unwrap();
        assert!(status.success());
        assert_eq!(child.try_wait().unwrap(), Some(status));
        drop(child);
        assert_reaped(pid);
    }

    #[test]
    fn actual_calloop_completion_transfers_child_ownership_to_root() {
        let (worker, channel) = SpawnWorker::new().unwrap();
        let mut event_loop = EventLoop::<Vec<SpawnOutcome>>::try_new().unwrap();
        event_loop
            .handle()
            .insert_source(channel, |event, _, outcomes| {
                if let Event::Msg(outcome) = event {
                    outcomes.push(outcome);
                }
            })
            .unwrap();
        worker.try_spawn(job(41, "sleep")).unwrap();
        let mut outcomes = Vec::new();
        let deadline = Instant::now() + Duration::from_secs(3);
        while outcomes.is_empty() {
            assert!(Instant::now() < deadline);
            event_loop
                .dispatch(Duration::from_millis(50), &mut outcomes)
                .unwrap();
        }
        let outcome = outcomes.pop().unwrap();
        assert_eq!(outcome.operation, 41);
        let mut child = outcome.result.unwrap();
        assert!(child.try_wait().unwrap().is_none());
        let pid = child.id();
        worker.shutdown().unwrap();
        assert!(child.try_wait().unwrap().is_none());
        drop(child);
        assert_reaped(pid);
    }

    #[test]
    fn eight_queued_jobs_reject_overflow_and_shutdown_cancels_active_result() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (pid_tx, pid_rx) = mpsc::channel();
        let (worker, channel) = SpawnWorker::with_launcher(move |request| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let child = launch(request)?;
            pid_tx.send(child.id()).unwrap();
            Ok(child)
        })
        .unwrap();
        worker.try_spawn(job(0, "sleep")).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        for operation in 1..=QUEUE_CAPACITY as u64 {
            worker.try_spawn(job(operation, "sleep")).unwrap();
        }
        assert_eq!(worker.try_spawn(job(100, "sleep")), Err(Error::QueueFull));
        let control = worker.control.clone();
        let shutdown = thread::spawn(move || worker.shutdown().unwrap());
        until(|| control.stopped());
        release_tx.send(()).unwrap();
        let pid = pid_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        shutdown.join().unwrap();
        assert_reaped(pid);
        assert!(matches!(
            channel.try_recv(),
            Err(mpsc::TryRecvError::Disconnected)
        ));
    }

    #[test]
    fn receiver_drop_kills_unsent_child_and_stops_worker_without_orphan() {
        let (entered_tx, entered_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let (pid_tx, pid_rx) = mpsc::channel();
        let (worker, channel) = SpawnWorker::with_launcher(move |request| {
            entered_tx.send(()).unwrap();
            release_rx.recv().unwrap();
            let child = launch(request)?;
            pid_tx.send(child.id()).unwrap();
            Ok(child)
        })
        .unwrap();
        worker.try_spawn(job(1, "sleep")).unwrap();
        entered_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        drop(channel);
        release_tx.send(()).unwrap();
        let pid = pid_rx.recv_timeout(Duration::from_secs(1)).unwrap();
        until(|| worker.is_stopped());
        worker.shutdown().unwrap();
        assert_reaped(pid);
    }

    #[test]
    fn bounded_completion_backlog_cannot_deadlock_worker_shutdown() {
        let (pid_tx, pid_rx) = mpsc::channel();
        let (worker, channel) = SpawnWorker::with_launcher(move |request| {
            let child = launch(request)?;
            pid_tx.send(child.id()).unwrap();
            Ok(child)
        })
        .unwrap();
        for operation in 0..=COMPLETION_CAPACITY as u64 {
            let mut request = Some(job(operation, "sleep"));
            until(
                || match worker.try_spawn(request.as_ref().unwrap().clone()) {
                    Ok(()) => {
                        request.take();
                        true
                    }
                    Err(Error::QueueFull) => false,
                    Err(error) => panic!("{error}"),
                },
            );
        }
        let pids = (0..=COMPLETION_CAPACITY)
            .map(|_| pid_rx.recv_timeout(Duration::from_secs(1)).unwrap())
            .collect::<Vec<_>>();
        for operation in 100..100 + QUEUE_CAPACITY as u64 {
            worker.try_spawn(job(operation, "sleep")).unwrap();
        }
        assert!(pid_rx.recv_timeout(Duration::from_millis(30)).is_err());
        let started = Instant::now();
        worker.shutdown().unwrap();
        assert!(started.elapsed() < Duration::from_secs(2));
        // Completed channel messages already belong to root; dropping that channel
        // outside dispatch kills/reaps them. The ninth active result was not sent.
        drop(channel);
        for pid in pids {
            assert_reaped(pid);
        }
    }
    #[test]
    fn failed_exec_reports_operation_and_worker_keeps_servicing_jobs() {
        let (worker, receiver) = SpawnWorker::new().unwrap();
        let mut missing = job(99, "exit");
        missing.argv[0] = format!("/flux-process-no-executable-{}", std::process::id());
        worker.try_spawn(missing).unwrap();
        worker.try_spawn(job(100, "exit")).unwrap();
        let mut outcomes = Vec::new();
        until(|| {
            while let Ok(outcome) = receiver.try_recv() {
                outcomes.push(outcome);
            }
            outcomes.len() == 2
        });
        let failed = outcomes.remove(0);
        assert_eq!(failed.operation, 99);
        assert!(!failed.result.unwrap_err().is_empty());
        let success = outcomes.remove(0);
        assert_eq!(success.operation, 100);
        let mut child = success.result.unwrap();
        assert!(child.wait().unwrap().success());
        assert!(!worker.is_stopped());
        worker.shutdown().unwrap();
    }
}
