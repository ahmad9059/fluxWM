use flux_ipc::{ErrorBody, ErrorCode, Response};
use std::{
    os::unix::fs::DirBuilderExt,
    process::Command,
    time::{Duration, Instant},
};

fn exercise(error: Option<ErrorCode>) -> std::process::Output {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "fluxctl-test-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
    ));
    std::fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    let mut server = flux_ipc::Server::bind(&dir, "cli").unwrap();
    let socket = server.socket_path().to_owned();
    let thread = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(3);
        let mut served = false;
        while !served && Instant::now() < deadline {
            server.poll(Instant::now(), |request| {
                served = true;
                match error {
                    None => Response::success(request.id, 7, serde_json::json!({"version":"test"})),
                    Some(code) => {
                        Response::error(request.id, 7, ErrorBody::new(code, "deliberate rejection"))
                    }
                }
            });
            std::thread::sleep(Duration::from_millis(1));
        }
        assert!(served);
    });
    let output = Command::new(env!("CARGO_BIN_EXE_fluxctl"))
        .args(["--socket", socket.to_str().unwrap(), "query", "version"])
        .output()
        .unwrap();
    thread.join().unwrap();
    std::fs::remove_dir_all(dir).unwrap();
    output
}
#[test]
fn cli_success_contains_sequence_and_payload() {
    let out = exercise(None);
    assert!(out.status.success(), "{:?}", out);
    let value: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(value["state_sequence"], "7");
    assert_eq!(value["result"]["version"], "test");
}
#[test]
fn cli_readiness_and_rejected_statuses() {
    assert_eq!(exercise(Some(ErrorCode::NotReady)).status.code(), Some(3));
    assert_eq!(
        exercise(Some(ErrorCode::PermissionDenied)).status.code(),
        Some(4)
    );
}
#[test]
fn help_and_missing_socket() {
    assert!(
        Command::new(env!("CARGO_BIN_EXE_fluxctl"))
            .arg("--help")
            .status()
            .unwrap()
            .success()
    );
    assert_eq!(
        Command::new(env!("CARGO_BIN_EXE_fluxctl"))
            .env_remove("FLUXWM_SOCKET")
            .args(["query", "version"])
            .output()
            .unwrap()
            .status
            .code(),
        Some(3)
    );
}
