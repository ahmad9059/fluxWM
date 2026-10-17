//! Worker-owned evaluation of already-acquired source snapshots.
use crate::{CompiledConfig, ConfigError, HelperLimits, MAX_SOURCE_BYTES};
use std::{
    path::PathBuf,
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
};

/// Checked, coordinator-local request identity. Zero is never issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct ConfigGeneration(u64);
impl ConfigGeneration {
    pub fn get(self) -> u64 {
        self.0
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigRequestError {
    #[error("configuration coordinator is shut down")]
    Closed,
    #[error("configuration worker stopped unexpectedly")]
    WorkerStopped,
    #[error("configuration request generation exhausted")]
    GenerationExhausted,
    #[error("configuration source exceeds 1 MiB")]
    SourceTooLarge,
    #[error("configuration source is not UTF-8")]
    SourceNotUtf8,
}
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigCommitError {
    #[error("configuration coordinator is shut down")]
    Closed,
    #[error("configuration candidate belongs to another coordinator")]
    ForeignCandidate,
    #[error("configuration candidate has been superseded")]
    Stale,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigFailureKind {
    Invalid,
    Io,
    Timeout,
    Helper,
    Cancelled,
}
/// One bounded diagnostic. No source snapshot or diagnostic history is retained.
#[derive(Debug)]
pub struct ConfigEvaluationFailure {
    pub kind: ConfigFailureKind,
    pub diagnostic: String,
}
impl From<ConfigError> for ConfigEvaluationFailure {
    fn from(error: ConfigError) -> Self {
        let kind = match &error {
            ConfigError::Invalid(_) => ConfigFailureKind::Invalid,
            ConfigError::Io(_) => ConfigFailureKind::Io,
            ConfigError::Timeout => ConfigFailureKind::Timeout,
            ConfigError::Helper { .. } => ConfigFailureKind::Helper,
            ConfigError::Cancelled => ConfigFailureKind::Cancelled,
        };
        // Helper errors, regex diagnostics and serde errors may quote arbitrary
        // source-controlled text. Retain only static stage/class information;
        // the synchronous API remains available to explicit diagnostic tooling.
        let diagnostic = match kind {
            ConfigFailureKind::Invalid => "configuration result failed schema or value validation",
            ConfigFailureKind::Io => "configuration helper could not be started or supervised",
            ConfigFailureKind::Timeout => "configuration helper exceeded its evaluation deadline",
            ConfigFailureKind::Helper => {
                "configuration helper rejected evaluation or exited unsuccessfully"
            }
            ConfigFailureKind::Cancelled => "configuration evaluation was cancelled",
        }
        .to_owned();
        Self { kind, diagnostic }
    }
}
/// An evaluated value, not an applied configuration. Intentionally not Clone.
#[derive(Debug)]
pub struct ConfigCandidate {
    generation: ConfigGeneration,
    owner: Weak<Shared>,
    config: Arc<CompiledConfig>,
}
impl ConfigCandidate {
    pub fn generation(&self) -> ConfigGeneration {
        self.generation
    }
    pub fn config(&self) -> &CompiledConfig {
        &self.config
    }
}
#[derive(Debug)]
pub struct ConfigEvaluation {
    pub generation: ConfigGeneration,
    pub result: Result<ConfigCandidate, ConfigEvaluationFailure>,
}
#[derive(Debug, Clone, Copy)]
pub struct ConfigCoordinatorStatus {
    pub latest: Option<ConfigGeneration>,
    pub active: Option<ConfigGeneration>,
    pub pending: Option<ConfigGeneration>,
    /// Observation only; signalling uses the supervisor's unreaped Child handle.
    pub active_helper_pid: Option<u32>,
    pub completion_ready: bool,
    pub closed: bool,
}
#[derive(Debug)]
struct Job {
    generation: ConfigGeneration,
    source: Vec<u8>,
}
#[derive(Debug, Default)]
struct State {
    latest: u64,
    active: Option<ConfigGeneration>,
    active_helper_pid: Option<u32>,
    pending: Option<Job>,
    completion: Option<ConfigEvaluation>,
    closed: bool,
}
#[derive(Debug, Default)]
struct Shared {
    state: Mutex<State>,
    changed: Condvar,
    cancelled: AtomicBool,
}

/// One worker, one active evaluation, one replaceable pending source, and one
/// completion slot. All public mutation needs exclusive caller ownership.
///
/// `notify` runs on the worker, outside the mutex, and must be nonblocking and
/// nonpanicking (for example a coalesced event-loop ping). Notifications are hints:
/// stale work may already have been superseded before the caller handles a wake.
/// This crate does not read source files or depend on any event-loop implementation.
/// Call shutdown/Drop outside compositor dispatch: joining, exec and kill/wait have
/// no universal wall-clock bound under kernel/filesystem failure.
pub struct ConfigCoordinator {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    last_good: Option<Arc<CompiledConfig>>,
}
impl ConfigCoordinator {
    pub fn new(
        helper: PathBuf,
        limits: HelperLimits,
        notify: impl Fn() + Send + 'static,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let worker_shared = shared.clone();
        let worker = thread::Builder::new()
            .name("flux-config".into())
            .spawn(move || {
                loop {
                    let job = {
                        let mut state = worker_shared
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        while !state.closed && state.pending.is_none() {
                            state = worker_shared
                                .changed
                                .wait(state)
                                .unwrap_or_else(|e| e.into_inner());
                        }
                        if state.closed {
                            return;
                        }
                        let job = state.pending.take().expect("pending work after wait");
                        state.active = Some(job.generation);
                        job
                    };
                    let result = crate::supervisor::load_with_helper_cancellable(
                        &helper,
                        &job.source,
                        limits,
                        &worker_shared.cancelled,
                        |pid| {
                            let mut state = worker_shared
                                .state
                                .lock()
                                .unwrap_or_else(|e| e.into_inner());
                            state.active_helper_pid = Some(pid);
                        },
                    )
                    .map(|config| ConfigCandidate {
                        generation: job.generation,
                        owner: Arc::downgrade(&worker_shared),
                        config: Arc::new(config),
                    })
                    .map_err(ConfigEvaluationFailure::from);
                    let publish = {
                        let mut state = worker_shared
                            .state
                            .lock()
                            .unwrap_or_else(|e| e.into_inner());
                        state.active = None;
                        state.active_helper_pid = None;
                        if !state.closed && state.latest == job.generation.0 {
                            state.completion = Some(ConfigEvaluation {
                                generation: job.generation,
                                result,
                            });
                            true
                        } else {
                            false
                        }
                    };
                    if publish {
                        notify();
                    }
                }
            })?;
        Ok(Self {
            shared,
            worker: Some(worker),
            last_good: None,
        })
    }
    /// Admit an owned snapshot without file I/O or helper execution. Invalid bytes
    /// are rejected without advancing generation or superseding accepted work.
    /// An accepted Lua/schema/helper failure DOES supersede older candidates.
    pub fn request(&mut self, source: Vec<u8>) -> Result<ConfigGeneration, ConfigRequestError> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(ConfigRequestError::Closed);
        }
        if self
            .worker
            .as_ref()
            .is_none_or(|worker| worker.is_finished())
        {
            return Err(ConfigRequestError::WorkerStopped);
        }
        if source.len() > MAX_SOURCE_BYTES {
            return Err(ConfigRequestError::SourceTooLarge);
        }
        if std::str::from_utf8(&source).is_err() {
            return Err(ConfigRequestError::SourceNotUtf8);
        }
        let next = state
            .latest
            .checked_add(1)
            .ok_or(ConfigRequestError::GenerationExhausted)?;
        let generation = ConfigGeneration(next);
        state.latest = next;
        state.completion = None;
        state.pending = Some(Job { generation, source });
        self.shared.changed.notify_one();
        Ok(generation)
    }
    /// Consume only a current completion. Merely evaluating successfully never
    /// changes last_good. The caller must bound storage of externally held candidates.
    pub fn take_completion(&mut self) -> Option<ConfigEvaluation> {
        let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return None;
        }
        state
            .completion
            .take()
            .filter(|value| value.generation.0 == state.latest)
    }
    /// Recheck current identity and generation, then explicitly retain last-good.
    /// Consumes a non-cloneable candidate even on rejection. Caller publication
    /// should occur at its own transaction boundary, without interleaved requests.
    pub fn commit(
        &mut self,
        candidate: ConfigCandidate,
    ) -> Result<Arc<CompiledConfig>, ConfigCommitError> {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        if state.closed {
            return Err(ConfigCommitError::Closed);
        }
        if !Weak::ptr_eq(&candidate.owner, &Arc::downgrade(&self.shared)) {
            return Err(ConfigCommitError::ForeignCandidate);
        }
        if candidate.generation.0 != state.latest {
            return Err(ConfigCommitError::Stale);
        }
        self.last_good = Some(candidate.config.clone());
        Ok(candidate.config)
    }
    pub fn last_good(&self) -> Option<&Arc<CompiledConfig>> {
        self.last_good.as_ref()
    }
    pub fn status(&self) -> ConfigCoordinatorStatus {
        let state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
        ConfigCoordinatorStatus {
            latest: (state.latest != 0).then_some(ConfigGeneration(state.latest)),
            active: state.active,
            pending: state.pending.as_ref().map(|job| job.generation),
            active_helper_pid: state.active_helper_pid,
            completion_ready: state.completion.is_some(),
            closed: state.closed,
        }
    }
    /// Stops admission and publication before cancelling/reaping and joining.
    /// Idempotent; retained last-good remains inspectable but cannot be recommitted.
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        {
            let mut state = self.shared.state.lock().unwrap_or_else(|e| e.into_inner());
            state.closed = true;
            state.pending = None;
            state.completion = None;
            self.shared.cancelled.store(true, Ordering::Release);
            self.shared.changed.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("configuration worker panicked"))?;
        }
        Ok(())
    }
}
impl Drop for ConfigCoordinator {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn generation_exhaustion_and_invalid_admission_do_not_alias() {
        let mut coordinator =
            ConfigCoordinator::new("/unused".into(), HelperLimits::default(), || {}).unwrap();
        coordinator.shared.state.lock().unwrap().latest = u64::MAX;
        assert_eq!(
            coordinator.request(vec![255]),
            Err(ConfigRequestError::SourceNotUtf8)
        );
        assert_eq!(
            coordinator.request(vec![b'x'; MAX_SOURCE_BYTES + 1]),
            Err(ConfigRequestError::SourceTooLarge)
        );
        assert_eq!(
            coordinator.request(b"return {}".to_vec()),
            Err(ConfigRequestError::GenerationExhausted)
        );
        assert_eq!(coordinator.status().latest.unwrap().get(), u64::MAX);
        assert!(coordinator.status().pending.is_none());
        coordinator.shutdown().unwrap();
        assert_eq!(
            coordinator.request(Vec::new()),
            Err(ConfigRequestError::Closed)
        );
    }
    #[test]
    fn diagnostics_never_retain_source_controlled_values() {
        for error in [
            crate::invalid("é".repeat(crate::MAX_DIAGNOSTIC_BYTES)),
            ConfigError::Helper {
                status: "exit status: 1".into(),
                diagnostic: "SECRET_SOURCE_LITERAL".into(),
            },
        ] {
            let failure = ConfigEvaluationFailure::from(error);
            assert!(failure.diagnostic.len() <= crate::MAX_DIAGNOSTIC_BYTES);
            assert!(!failure.diagnostic.contains("SECRET_SOURCE_LITERAL"));
            assert!(!failure.diagnostic.contains('é'));
        }
    }
    #[test]
    #[cfg(target_os = "linux")]
    fn notification_can_inspect_shared_state_without_internal_mutex() {
        use std::sync::mpsc;
        let (sender, receiver) = mpsc::sync_channel(1);
        let observer = Arc::new(Mutex::new(None::<Weak<Shared>>));
        let observed = observer.clone();
        let mut coordinator = ConfigCoordinator::new(
            "/missing/flux-helper".into(),
            HelperLimits::default(),
            move || {
                if let Some(shared) = observed.lock().unwrap().as_ref().and_then(Weak::upgrade) {
                    let state = shared
                        .state
                        .try_lock()
                        .expect("notify called with coordinator lock held");
                    assert!(state.completion.is_some());
                }
                sender.try_send(()).unwrap();
            },
        )
        .unwrap();
        *observer.lock().unwrap() = Some(Arc::downgrade(&coordinator.shared));
        coordinator
            .request(b"return {schema_version=1}".to_vec())
            .unwrap();
        receiver
            .recv_timeout(std::time::Duration::from_secs(2))
            .unwrap();
        assert_eq!(
            coordinator
                .take_completion()
                .unwrap()
                .result
                .unwrap_err()
                .kind,
            ConfigFailureKind::Io
        );
        coordinator.shutdown().unwrap();
    }
}
