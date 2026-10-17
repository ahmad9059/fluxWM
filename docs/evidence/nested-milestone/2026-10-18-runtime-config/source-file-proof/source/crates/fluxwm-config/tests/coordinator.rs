#![cfg(target_os = "linux")]
use flux_config::{
    ConfigCandidate, ConfigCommitError, ConfigCoordinator, ConfigEvaluation, ConfigFailureKind,
    ConfigGeneration, ConfigRequestError, HelperLimits,
};
use std::{
    path::PathBuf,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

fn coordinator() -> ConfigCoordinator {
    ConfigCoordinator::new(
        PathBuf::from(env!("CARGO_BIN_EXE_fluxwm-config")),
        HelperLimits {
            deadline: Duration::from_millis(250),
            ..Default::default()
        },
        || {},
    )
    .unwrap()
}
fn source(gaps: u32) -> Vec<u8> {
    format!("return {{schema_version=1,layout={{gaps={gaps}}}}}").into_bytes()
}
fn await_result(loader: &mut ConfigCoordinator) -> ConfigEvaluation {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Some(value) = loader.take_completion() {
            return value;
        }
        assert!(
            Instant::now() < deadline,
            "no completion: {:?}",
            loader.status()
        );
        thread::sleep(Duration::from_millis(2));
    }
}
fn await_active(loader: &ConfigCoordinator, generation: ConfigGeneration) {
    let deadline = Instant::now() + Duration::from_secs(1);
    while loader.status().active != Some(generation) {
        assert!(Instant::now() < deadline, "evaluation never became active");
        thread::yield_now();
    }
}
fn await_helper(loader: &ConfigCoordinator) -> u32 {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        if let Some(pid) = loader.status().active_helper_pid {
            return pid;
        }
        assert!(Instant::now() < deadline, "actual helper never spawned");
        thread::yield_now();
    }
}
fn assert_reaped(pid: u32) {
    assert!(
        std::path::Path::new("/proc/self").exists(),
        "Linux procfs is required for helper retirement evidence"
    );
    // A live child OR unreaped zombie would still have a procfs entry. The
    // supervisor unit test additionally checks ECHILD using owned-child waitpid.
    assert!(
        !std::path::Path::new(&format!("/proc/{pid}")).exists(),
        "helper is still live, unreaped, or its numerical PID has been reused"
    );
}
fn candidate(loader: &mut ConfigCoordinator, gaps: u32) -> ConfigCandidate {
    loader.request(source(gaps)).unwrap();
    await_result(loader).result.unwrap()
}
const CATCHING_LOOP: &[u8] = b"while true do pcall(function() while true do end end) end";

#[test]
fn actual_helper_coalesces_pending_and_rejects_stale_even_when_latest_fails() {
    let mut loader = coordinator();
    let first = candidate(&mut loader, 11);
    assert!(loader.last_good().is_none(), "evaluation must not commit");
    loader.commit(first).unwrap();
    let held = candidate(&mut loader, 12);
    let invalid = loader
        .request(b"return {schema_version=1,unknown=true}".to_vec())
        .unwrap();
    let failure = await_result(&mut loader);
    assert_eq!(failure.generation, invalid);
    assert_eq!(failure.result.unwrap_err().kind, ConfigFailureKind::Helper);
    assert_eq!(loader.status().latest, Some(invalid));
    assert_eq!(loader.commit(held).unwrap_err(), ConfigCommitError::Stale);
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 11.);

    let active = loader.request(CATCHING_LOOP.to_vec()).unwrap();
    await_active(&loader, active);
    let stale_helper = await_helper(&loader);
    let replaced = loader.request(source(22)).unwrap();
    let latest = loader.request(source(33)).unwrap();
    assert!(active < replaced && replaced < latest);
    assert_eq!(loader.status().active, Some(active));
    assert_eq!(loader.status().pending, Some(latest));
    let value = await_result(&mut loader);
    assert_eq!(value.generation, latest);
    assert_reaped(stale_helper);
    let value = value.result.unwrap();
    assert_eq!(value.config().config.layout.gaps, 33.);
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 11.);
    loader.commit(value).unwrap();
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 33.);
    assert!(loader.take_completion().is_none());
    loader.shutdown().unwrap();
}
#[test]
fn actual_helper_timeout_then_recovery_and_foreign_candidate_rejection() {
    let mut loader = coordinator();
    loader.request(CATCHING_LOOP.to_vec()).unwrap();
    let timed_out_helper = await_helper(&loader);
    assert_eq!(
        await_result(&mut loader).result.unwrap_err().kind,
        ConfigFailureKind::Timeout
    );
    assert_reaped(timed_out_helper);
    let good = candidate(&mut loader, 17);
    assert!(loader.last_good().is_none());
    let mut other = coordinator();
    other.request(source(18)).unwrap();
    assert_eq!(
        other.commit(good).unwrap_err(),
        ConfigCommitError::ForeignCandidate
    );
    let own = await_result(&mut other).result.unwrap();
    other.commit(own).unwrap();
    assert_eq!(other.last_good().unwrap().config.layout.gaps, 18.);
    loader.shutdown().unwrap();
    other.shutdown().unwrap();
}
#[test]
fn actual_helper_shutdown_cancels_active_drops_pending_and_prevents_late_commit() {
    let (sender, receiver) = mpsc::sync_channel(1);
    let mut loader = ConfigCoordinator::new(
        PathBuf::from(env!("CARGO_BIN_EXE_fluxwm-config")),
        HelperLimits {
            deadline: Duration::from_secs(10),
            ..Default::default()
        },
        move || {
            let _ = sender.try_send(());
        },
    )
    .unwrap();
    let held = candidate(&mut loader, 10);
    receiver.recv_timeout(Duration::from_secs(1)).unwrap();
    let active = loader.request(CATCHING_LOOP.to_vec()).unwrap();
    await_active(&loader, active);
    let cancelled_helper = await_helper(&loader);
    loader.request(source(40)).unwrap();
    let started = Instant::now();
    loader.shutdown().unwrap();
    assert_reaped(cancelled_helper);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "cancellation did not finish below the two-second CPU backstop"
    );
    assert!(
        loader.status().closed
            && loader.status().active.is_none()
            && loader.status().pending.is_none()
    );
    assert!(loader.take_completion().is_none());
    assert_eq!(loader.commit(held).unwrap_err(), ConfigCommitError::Closed);
    assert_eq!(loader.request(source(50)), Err(ConfigRequestError::Closed));
    assert!(
        receiver.try_recv().is_err(),
        "shutdown published a late completion"
    );
    loader.shutdown().unwrap();
}

#[test]
fn actual_lua_error_text_is_not_retained_in_coordinator_diagnostics() {
    let mut loader = coordinator();
    loader
        .request(b"error('SECRET_SOURCE_LITERAL')".to_vec())
        .unwrap();
    let failure = await_result(&mut loader).result.unwrap_err();
    assert_eq!(failure.kind, ConfigFailureKind::Helper);
    assert!(!failure.diagnostic.contains("SECRET_SOURCE_LITERAL"));
    loader.shutdown().unwrap();
}

#[test]
fn coordinator_shutdown_cancels_sleeping_helper_without_cpu_backstop() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let expected_sleep = fs::canonicalize("/bin/sleep").expect("sleep executable is required");
    let sleep_metadata = fs::metadata(&expected_sleep).unwrap();
    assert!(
        sleep_metadata.is_file() && sleep_metadata.permissions().mode() & 0o111 != 0,
        "sleep must be an existing executable regular file"
    );
    struct Fixture(PathBuf);
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let fixture = Fixture(std::env::temp_dir().join(format!(
        "flux-coordinator-cancellation-{}-{unique}",
        std::process::id()
    )));
    fs::create_dir(&fixture.0).unwrap();
    let helper = fixture.0.join("helper");
    fs::write(&helper, "#!/bin/sh\nexec /bin/sleep 30\n").unwrap();
    fs::set_permissions(&helper, fs::Permissions::from_mode(0o700)).unwrap();
    let mut loader = ConfigCoordinator::new(
        helper,
        HelperLimits {
            deadline: Duration::from_secs(10),
            ..Default::default()
        },
        || {},
    )
    .unwrap();
    let active = loader.request(source(10)).unwrap();
    let pid = await_helper(&loader);
    let child_exe = PathBuf::from(format!("/proc/{pid}/exe"));
    let ready_deadline = Instant::now() + Duration::from_secs(1);
    loop {
        let status = loader.status();
        assert_eq!(
            status.active,
            Some(active),
            "helper exited before sleep readiness"
        );
        assert_eq!(
            status.active_helper_pid,
            Some(pid),
            "active helper identity changed"
        );
        if fs::read_link(&child_exe).is_ok_and(|image| image == expected_sleep) {
            break;
        }
        assert!(
            Instant::now() < ready_deadline,
            "owned helper never entered the expected sleep image"
        );
        thread::sleep(Duration::from_millis(2));
    }
    loader.request(source(20)).unwrap();
    // A spawned PID alone is insufficient: an interpreter/exec failure could
    // otherwise make shutdown look fast without exercising cancellation.
    assert_eq!(loader.status().active, Some(active));
    assert_eq!(loader.status().active_helper_pid, Some(pid));
    assert_eq!(fs::read_link(&child_exe).unwrap(), expected_sleep);
    let started = Instant::now();
    loader.shutdown().unwrap();
    assert_reaped(pid);
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "shutdown waited for the ten-second evaluation deadline instead of cancelling the sleeping helper"
    );
    assert!(loader.status().closed && loader.status().pending.is_none());
    assert!(loader.take_completion().is_none());
}

struct SourceDirectory(PathBuf);
impl SourceDirectory {
    fn new() -> Self {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "flux-config-source-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for SourceDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn worker_reads_real_file_and_explicitly_allows_symlinks_and_non_utf8_paths() {
    use std::{
        fs,
        os::unix::{ffi::OsStringExt, fs::symlink},
    };
    let directory = SourceDirectory::new();
    let path = directory
        .0
        .join(std::ffi::OsString::from_vec(b"config-\xff.lua".to_vec()));
    let mut largest_valid = source(27);
    largest_valid.resize(flux_config::MAX_SOURCE_BYTES, b' ');
    fs::write(&path, largest_valid).unwrap();
    let link = directory.0.join("config-link.lua");
    symlink(&path, &link).unwrap();
    let mut loader = coordinator();
    for path in [path, link] {
        let generation = loader.request_path(path).unwrap();
        let result = await_result(&mut loader);
        assert_eq!(result.generation, generation);
        let evaluated = result.result.unwrap();
        assert_eq!(evaluated.config().config.layout.gaps, 27.);
        assert!(
            loader.last_good().is_none(),
            "file evaluation must not implicitly commit"
        );
    }
    loader.shutdown().unwrap();
}

#[test]
fn accepted_missing_path_supersedes_held_candidate_and_recovers_without_path_diagnostics() {
    let directory = SourceDirectory::new();
    let mut loader = coordinator();
    let original = candidate(&mut loader, 11);
    loader.commit(original).unwrap();
    let held = candidate(&mut loader, 12);
    let missing = directory.0.join("SECRET_USER_PATH.lua");
    let generation = loader.request_path(missing.clone()).unwrap();
    let result = await_result(&mut loader);
    assert_eq!(result.generation, generation);
    let failure = result.result.unwrap_err();
    assert_eq!(failure.kind, ConfigFailureKind::Io);
    assert!(!failure.diagnostic.contains("SECRET_USER_PATH"));
    assert!(!failure.diagnostic.contains(missing.to_str().unwrap()));
    assert_eq!(loader.commit(held).unwrap_err(), ConfigCommitError::Stale);
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 11.);
    let good = directory.0.join("good.lua");
    std::fs::write(&good, source(32)).unwrap();
    loader.request_path(good).unwrap();
    let recovered = await_result(&mut loader).result.unwrap();
    loader.commit(recovered).unwrap();
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 32.);
    loader.shutdown().unwrap();
}

#[test]
fn worker_rejects_directory_oversized_and_non_utf8_source_before_helper_evaluation() {
    use std::fs;
    let directory = SourceDirectory::new();
    let oversized = directory.0.join("large.lua");
    fs::File::create(&oversized)
        .unwrap()
        .set_len((flux_config::MAX_SOURCE_BYTES + 1) as u64)
        .unwrap();
    let invalid = directory.0.join("invalid.lua");
    fs::write(&invalid, [255]).unwrap();
    let mut loader = coordinator();
    for path in [directory.0.clone(), oversized, invalid] {
        let generation = loader.request_path(path).unwrap();
        let result = await_result(&mut loader);
        assert_eq!(result.generation, generation);
        // Sending these bytes to the Lua helper would instead produce Helper;
        // Invalid here establishes rejection during worker source acquisition.
        assert_eq!(result.result.unwrap_err().kind, ConfigFailureKind::Invalid);
        assert!(loader.status().active_helper_pid.is_none());
    }
    let recovered = candidate(&mut loader, 13);
    assert_eq!(recovered.config().config.layout.gaps, 13.);
    loader.shutdown().unwrap();
}

#[test]
fn invalid_path_admission_preserves_generation_and_held_candidate() {
    use std::os::unix::ffi::OsStringExt;
    let mut loader = coordinator();
    let held = candidate(&mut loader, 15);
    let generation = held.generation();
    for path in [
        PathBuf::new(),
        "relative.lua".into(),
        PathBuf::from(format!(
            "/{}",
            "x".repeat(flux_config::MAX_SOURCE_PATH_BYTES)
        )),
        PathBuf::from(std::ffi::OsString::from_vec(b"/has\0nul".to_vec())),
    ] {
        assert_eq!(
            loader.request_path(path),
            Err(ConfigRequestError::InvalidPath)
        );
        assert_eq!(loader.status().latest, Some(generation));
    }
    loader.commit(held).unwrap();
    assert_eq!(loader.last_good().unwrap().config.layout.gaps, 15.);
    loader.shutdown().unwrap();
}

#[test]
fn fifo_path_worker_child() {
    let Some(path) = std::env::var_os("FLUX_CONFIG_FIFO_TEST_PATH") else {
        return;
    };
    let mut loader = coordinator();
    let started = Instant::now();
    loader.request_path(PathBuf::from(path)).unwrap();
    let failure = await_result(&mut loader).result.unwrap_err();
    assert_eq!(failure.kind, ConfigFailureKind::Invalid);
    assert!(started.elapsed() < Duration::from_secs(1));
    assert!(loader.status().active_helper_pid.is_none());
    loader.shutdown().unwrap();
}

#[test]
fn fifo_without_writer_does_not_block_and_test_process_is_owned_and_reaped() {
    use std::process::{Child, Command};
    struct OwnedTest(Child);
    impl Drop for OwnedTest {
        fn drop(&mut self) {
            if self.0.try_wait().ok().flatten().is_none() {
                let _ = self.0.kill();
            }
            let _ = self.0.wait();
        }
    }
    let directory = SourceDirectory::new();
    let fifo = directory.0.join("source.fifo");
    assert!(
        Command::new("mkfifo")
            .arg(&fifo)
            .status()
            .expect("coreutils mkfifo is required")
            .success()
    );
    let mut child = OwnedTest(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "fifo_path_worker_child", "--nocapture"])
            .env("FLUX_CONFIG_FIFO_TEST_PATH", &fifo)
            .spawn()
            .unwrap(),
    );
    let pid = child.0.id();
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        if let Some(status) = child.0.try_wait().unwrap() {
            assert!(status.success(), "FIFO worker subprocess failed");
            break;
        }
        if Instant::now() >= deadline {
            child.0.kill().unwrap();
            child.0.wait().unwrap();
            assert_reaped(pid);
            panic!("FIFO source worker blocked; owned subprocess was killed and reaped");
        }
        thread::sleep(Duration::from_millis(2));
    }
    assert_reaped(pid);
}
