#![cfg(feature = "system")]
use std::process::Command;
#[test]
fn read_only_check_does_not_need_a_session_backend() {
    let output = Command::new(env!("CARGO_BIN_EXE_fluxwm-native-probe"))
        .arg("--check")
        .env("LIBSEAT_BACKEND", "fluxwm-no-such-backend")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(data["session_opened"], false);
    assert_eq!(data["gpu_contexts_created"], 0);
    assert_eq!(data["native_compositor"], false);
}
#[test]
fn help_and_invalid_options_do_not_start_session_mode() {
    let binary = env!("CARGO_BIN_EXE_fluxwm-native-probe");
    let help = Command::new(binary).arg("--help").output().unwrap();
    assert!(help.status.success());
    assert!(String::from_utf8_lossy(&help.stdout).contains("--session"));
    let bad = Command::new(binary).arg("--session=bad").output().unwrap();
    assert!(!bad.status.success());
}
