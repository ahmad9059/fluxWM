//! Manual native initialization probe. This is not a compositor or modeset demo.
use flux_native::system::{Driver, Failure, NativeEvent};
use serde_json::json;
use smithay::reexports::calloop::{
    Dispatcher, EventLoop,
    signals::{Signal, Signals},
    timer::{TimeoutAction, Timer},
};
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
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    match args.as_slice() {
        [] => {
            help();
            return Ok(());
        }
        [arg] if arg == "--help" || arg == "-h" => {
            help();
            return Ok(());
        }
        [arg] if arg == "--check" => {
            println!(
                "{}",
                serde_json::to_string_pretty(
                    &json!({"scope":"read-only udev inventory; no native desktop qualification","session_opened":false,"gpu_contexts_created":0,"native_compositor":false,"devices":flux_native::system::device_ids("seat0")?})
                )?
            );
            return Ok(());
        }
        [arg] if arg == "--session" => (),
        _ => return Err("expected --check, --session or --help".into()),
    }
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
    let driver = Driver::new(
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
    let initialization_healthy = initialization_healthy(
        before.active,
        before.closed,
        gpus.len(),
        state.failure_events,
        close_failures,
    );
    println!(
        "{}",
        serde_json::to_string_pretty(
            &json!({"scope":"manual native initialization only; no frame or desktop qualification","native_compositor":false,"initialization_healthy":initialization_healthy,"event_loop_budget_ms":15000,"universal_shutdown_deadline":false,"before_stop":before,"after_stop":after,"prepared_gpus":gpus,"input_events":state.input_events,"gpu_preparations":state.gpu_preparations,"gpu_removals":state.gpu_removals,"session_transitions":state.session_transitions,"vblanks":state.vblanks,"failures":state.failures,"failure_events":state.failure_events,"failures_truncated":state.failure_events>32,"close_failures":close_failures})
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
        "fluxwm-native-probe --check | --session\n--check: read-only udev inventory; opens no seat/DRM/input devices.\n--session: manual physical-VT initialization, libinput and GPU context probe; exits after a 15-second event-loop interval.\nNo compositor output surfaces are created. Smithay may change/restore DRM state during initialization/teardown; use a separate physical VT. This is not the FluxWM native compositor. Kernel/library calls have no universal deadline."
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
    use super::initialization_healthy;
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
