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
