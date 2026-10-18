//! Manual native initialization probe. This is not a compositor or modeset demo.
use flux_native::system::{Driver, Failure, GpuInfo, NativeEvent};
use serde::Serialize;
use serde_json::json;
use smithay::reexports::calloop::{
    Dispatcher, EventLoop,
    signals::{Signal, Signals},
    timer::{TimeoutAction, Timer},
};
use std::ffi::OsString;
use std::time::Duration;
#[derive(Default)]
struct State {
    driver: Option<Driver<Self>>,
    input_events: u64,
    gpu_preparations: u64,
    gpu_removals: u64,
    session_transitions: u64,
    vblanks: u64,
    failures: Vec<Failure>,
    failure_events: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Command {
    Help,
    Check,
    Session { test_outputs: bool },
}
fn command(args: &[OsString]) -> Result<Command, &'static str> {
    match args {
        [] => Ok(Command::Help),
        [arg] if arg == "--help" || arg == "-h" => Ok(Command::Help),
        [arg] if arg == "--check" => Ok(Command::Check),
        [arg] if arg == "--session" => Ok(Command::Session {
            test_outputs: false,
        }),
        [first, second]
            if (first == "--session" && second == "--test-outputs")
                || (second == "--session" && first == "--test-outputs") =>
        {
            Ok(Command::Session { test_outputs: true })
        }
        _ => Err("expected --check, --session [--test-outputs] or --help"),
    }
}
#[derive(Serialize)]
struct OutputTestRecord {
    device: String,
    generation: String,
    session_epoch: String,
    report: flux_native::target::TestReport,
}
fn output_tests_healthy(requested: bool, records: &[OutputTestRecord], gpus: &[GpuInfo]) -> bool {
    if !requested {
        return true;
    }
    let current: std::collections::BTreeSet<_> = gpus
        .iter()
        .map(|gpu| (&gpu.device, &gpu.generation, &gpu.session_epoch))
        .collect();
    let tested: std::collections::BTreeSet<_> = records
        .iter()
        .map(|record| (&record.device, &record.generation, &record.session_epoch))
        .collect();
    !gpus.is_empty()
        && current.len() == gpus.len()
        && tested.len() == records.len()
        && current == tested
        && records.iter().all(|record| record.report.ok)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    let test_outputs = match command(&args)? {
        Command::Help => {
            help();
            return Ok(());
        }
        Command::Check => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"scope":"read-only udev inventory; no native desktop qualification","session_opened":false,"gpu_contexts_created":0,"native_compositor":false,"devices":flux_native::system::device_ids("seat0")?})
                )?
            );
            return Ok(());
        }
        Command::Session { test_outputs } => test_outputs,
    };
    // Own signal masking before GPU libraries can start helper threads. This
    // independent dispatcher outlives all native resources, including errors.
    let signals = Signals::new(&[Signal::SIGINT, Signal::SIGTERM])?;
    let mut event_loop = EventLoop::<State>::try_new()?;
    let stop = event_loop.get_signal();
    let signal_owner = Dispatcher::new(signals, move |_, _, _: &mut State| stop.stop());
    event_loop
        .handle()
        .register_dispatcher(signal_owner.clone())
        .map_err(|_| "failed to register signals")?;
    let mut state = State::default();
    let mut driver = Driver::new(
        &event_loop.handle(),
        &mut state,
        |event, state| match event {
            NativeEvent::Input(_) => state.input_events = state.input_events.saturating_add(1),
            NativeEvent::GpuReady(_) => {
                state.gpu_preparations = state.gpu_preparations.saturating_add(1)
            }
            NativeEvent::GpuRemoved(_) => state.gpu_removals = state.gpu_removals.saturating_add(1),
            NativeEvent::SessionActive(_) => {
                state.session_transitions = state.session_transitions.saturating_add(1)
            }
            NativeEvent::VBlank { .. } => state.vblanks = state.vblanks.saturating_add(1),
            NativeEvent::Failure { error, .. } => {
                state.failure_events = state.failure_events.saturating_add(1);
                if state.failures.len() < 32 {
                    state.failures.push(error);
                }
            }
        },
    )?;
    let mut output_tests = vec![];
    if test_outputs {
        for gpu in driver.gpu_info() {
            let report = driver.test_output_targets(gpu.device.parse()?);
            output_tests.push(OutputTestRecord {
                device: gpu.device,
                generation: gpu.generation,
                session_epoch: gpu.session_epoch,
                report,
            });
        }
    }
    state.driver = Some(driver);
    let stop = event_loop.get_signal();
    event_loop
        .handle()
        .insert_source(
            Timer::from_duration(Duration::from_secs(15)),
            move |_, _, _| {
                stop.stop();
                TimeoutAction::Drop
            },
        )
        .map_err(|_| "failed to register exit timer")?;
    event_loop.run(None, &mut state, |_| {})?;
    let mut driver = state.driver.take().ok_or("native driver missing")?;
    let before = driver.snapshot();
    let gpus = driver.gpu_info();
    driver.stop();
    let after = driver.snapshot();
    let close_failures = driver.close_failures();
    drop(driver);
    let output_tests_healthy = output_tests_healthy(test_outputs, &output_tests, &gpus);
    let initialization_healthy = output_tests_healthy
        && initialization_healthy(
            before.active,
            before.closed,
            gpus.len(),
            state.failure_events,
            close_failures,
        );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"scope":"manual native initialization only; no frame or desktop qualification","native_compositor":false,"initialization_healthy":initialization_healthy,"event_loop_budget_ms":15000,"universal_shutdown_deadline":false,"before_stop":before,"after_stop":after,"prepared_gpus":gpus,"output_tests_requested":test_outputs,"output_tests":output_tests,"output_tests_healthy":output_tests_healthy,"output_tests_scope":"initial ready-GPU snapshot only; final GPU tickets must still match; no automatic retest, frame or display qualification","input_events":state.input_events,"gpu_preparations":state.gpu_preparations,"gpu_removals":state.gpu_removals,"session_transitions":state.session_transitions,"vblanks":state.vblanks,"failures":state.failures,"failure_events":state.failure_events,"failures_truncated":state.failure_events>32,"close_failures":close_failures})
        )?
    );
    // Keep the dispatcher (and blocked termination signals) through cleanup.
    drop(event_loop);
    drop(signal_owner);
    if !initialization_healthy {
        return Err("native initialization or cleanup reported failures".into());
    }
    Ok(())
}
fn help() {
    println!(
        "fluxwm-native-probe --check | --session [--test-outputs]\n--check: read-only udev inventory; opens no seat/DRM/input devices.\n--session: manual physical-VT initialization, libinput and GPU context probe; exits after a 15-second event-loop interval.\n--test-outputs requires --session: allocate/render owned RGB targets and submit an atomic TEST_ONLY request, then release them. Legacy devices are rejected. This option is not executed by device-free checks.\nNo compositor output surfaces are created. Smithay may change/restore DRM state during initialization/teardown; use a separate physical VT. This is not the FluxWM native compositor. Kernel/library calls have no universal deadline."
    );
}

fn initialization_healthy(
    active: bool,
    closed: bool,
    gpu_count: usize,
    failures: u64,
    close_failures: u64,
) -> bool {
    active && !closed && gpu_count > 0 && failures == 0 && close_failures == 0
}
#[cfg(test)]
mod tests {
    use super::{Command, OutputTestRecord, command, initialization_healthy, output_tests_healthy};
    use flux_native::{system::GpuInfo, target::TestReport};
    #[test]
    fn target_test_command_requires_explicit_session_opt_in() {
        let args = |values: &[&str]| {
            values
                .iter()
                .map(std::ffi::OsString::from)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            command(&args(&["--session"])).unwrap(),
            Command::Session {
                test_outputs: false
            }
        );
        for pair in [
            ["--session", "--test-outputs"],
            ["--test-outputs", "--session"],
        ] {
            assert_eq!(
                command(&args(&pair)).unwrap(),
                Command::Session { test_outputs: true }
            );
        }
        for values in [
            vec!["--test-outputs"],
            vec!["--check", "--test-outputs"],
            vec!["--session", "--test-outputs", "--test-outputs"],
        ] {
            assert!(command(&args(&values)).is_err());
        }
    }
    #[test]
    fn target_health_requires_nonempty_successful_current_gpu_tickets() {
        let gpu = GpuInfo {
            device: "1".into(),
            generation: "2".into(),
            session_epoch: "3".into(),
            connectors: 1,
            crtcs: 1,
            import_formats: 1,
            render_formats: 1,
            output_plan: None,
            output_failure: None,
        };
        let mut record = OutputTestRecord {
            device: gpu.device.clone(),
            generation: gpu.generation.clone(),
            session_epoch: gpu.session_epoch.clone(),
            report: TestReport {
                ok: true,
                planned_targets: 1,
                unassigned_connectors: 0,
                attempts: 1,
                nominal_bytes: "4".into(),
                tested_targets: vec![],
                failure: None,
                cleanup_failures: "0".into(),
            },
        };
        assert!(output_tests_healthy(false, &[], &[]));
        assert!(!output_tests_healthy(true, &[], &[]));
        assert!(!output_tests_healthy(true, &[], std::slice::from_ref(&gpu)));
        assert!(output_tests_healthy(
            true,
            std::slice::from_ref(&record),
            std::slice::from_ref(&gpu)
        ));
        record.generation = "4".into();
        assert!(!output_tests_healthy(
            true,
            std::slice::from_ref(&record),
            std::slice::from_ref(&gpu)
        ));
        record.generation = gpu.generation.clone();
        record.report.ok = false;
        assert!(!output_tests_healthy(true, &[record], &[gpu]));
    }
    #[test]
    fn empty_or_inactive_initialization_is_not_healthy() {
        assert!(initialization_healthy(true, false, 1, 0, 0));
        for (active, closed, gpus, failures, closes) in [
            (false, false, 1, 0, 0),
            (true, true, 1, 0, 0),
            (true, false, 0, 0, 0),
            (true, false, 1, 1, 0),
            (true, false, 1, 0, 1),
        ] {
            assert!(!initialization_healthy(
                active, closed, gpus, failures, closes
            ));
        }
    }
}
