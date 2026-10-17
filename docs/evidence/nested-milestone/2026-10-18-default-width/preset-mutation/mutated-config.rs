//! Nested logical configuration: all source/evaluation/binding work is worker-owned.
use flux_config::{Config, ConfigCoordinator, HelperLimits};
use flux_input::{BindingSet, UnmappedPolicy};
use std::{
    path::PathBuf,
    sync::{Arc, Condvar, Mutex},
    thread::JoinHandle,
};
use xkbcommon::xkb;

type Failure = &'static str;
struct Prepared {
    desired: Config,
    bindings: BindingSet,
    default_width: flux_layout::WidthPolicy,
}
// Retired schema/tables and replaced live-model storage are dropped on the worker.
// Temporary layout clones in reconfigure/validation still drop on dispatch.
struct Retired {
    _desired: Option<Config>,
    _bindings: Option<BindingSet>,
    _layout: Option<flux_layout::Layout>,
}
#[derive(Default)]
struct Control {
    latest: u64,
    pending: Option<(u64, PathBuf)>,
    evaluation_ready: bool,
    completion: Option<(u64, Result<Prepared, Failure>)>,
    retired: Option<Retired>,
    closed: bool,
}
#[derive(Default)]
struct Shared {
    control: Mutex<Control>,
    changed: Condvar,
}
pub struct RuntimeConfig {
    source: PathBuf,
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
    desired: Option<Config>,
    applied: Option<u64>,
    status: &'static str,
    last_error: Option<Failure>,
}
impl RuntimeConfig {
    fn new(
        source: PathBuf,
        helper: PathBuf,
        keymap: String,
        ping: calloop::ping::Ping,
    ) -> std::io::Result<Self> {
        let shared = Arc::new(Shared::default());
        let notified = shared.clone();
        let coordinator = ConfigCoordinator::new(helper, HelperLimits::default(), move || {
            notified
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .evaluation_ready = true;
            notified.changed.notify_one();
        })?;
        let control = shared.clone();
        let worker = std::thread::Builder::new()
            .name("flux-config-prepare".into())
            .spawn(move || run_worker(control, coordinator, keymap, ping))?;
        Ok(Self {
            source,
            shared,
            worker: Some(worker),
            desired: None,
            applied: None,
            status: "pending",
            last_error: None,
        })
    }
    fn request(&mut self) -> Result<u64, Failure> {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if c.closed || self.worker.as_ref().is_none_or(|w| w.is_finished()) {
            return Err("configuration worker is unavailable");
        }
        let generation = c
            .latest
            .checked_add(1)
            .ok_or("configuration generation exhausted")?;
        c.latest = generation;
        // Only a bounded path is replaced here. Old large completions stay on
        // the worker until it observes the superseding request.
        c.pending = Some((generation, self.source.clone()));
        self.status = "pending";
        self.last_error = None;
        self.shared.changed.notify_one();
        Ok(generation)
    }
    fn completion(&mut self) -> Option<(u64, Result<Prepared, Failure>)> {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if self.status != "failed" && self.worker.as_ref().is_none_or(|w| w.is_finished()) {
            return Some((c.latest, Err("configuration preparation worker stopped")));
        }
        if c.retired.is_some() || c.completion.as_ref()?.0 != c.latest {
            return None;
        }
        c.completion.take()
    }
    fn retire(&self, value: Retired) {
        let mut c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        // Only dispatch fills this slot, immediately after taking a completion
        // which checked it empty. The worker can only empty it in between.
        assert!(c.retired.is_none());
        c.retired = Some(value);
        self.shared.changed.notify_one();
    }
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        {
            let mut c = self
                .shared
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner());
            c.closed = true;
            self.shared.changed.notify_one();
        }
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| std::io::Error::other("configuration preparation worker panicked"))?;
        }
        Ok(())
    }
    pub fn diagnostics(&self) -> serde_json::Value {
        let c = self
            .shared
            .control
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        serde_json::json!({
            "requested_generation":(c.latest != 0).then(||c.latest.to_string()),
            "applied_generation":self.applied.map(|g|g.to_string()),
            "prepared_generation":c.completion.as_ref().filter(|(g,r)|*g == c.latest && r.is_ok()).map(|(g,_)|g.to_string()),
            "status":self.status, "last_error":self.last_error,
        })
    }
    pub fn applied(&self) -> bool {
        self.applied.is_some()
    }
}
impl Drop for RuntimeConfig {
    fn drop(&mut self) {
        let _ = self.shutdown();
    }
}
fn resolve_default_width(
    layout: &flux_config::Layout,
) -> Result<flux_layout::WidthPolicy, Failure> {
    let width = match &layout.default_width {
        flux_config::Width::Preset { preset } => layout
            .width_presets
            .get(*preset)
            .ok_or("default width preset is invalid")?,
        width => width,
    };
    match width {
        flux_config::Width::Pixels { logical_px } => {
            Ok(flux_layout::WidthPolicy::Fixed(*logical_px))
        }
        flux_config::Width::Fraction { fraction } => {
            Ok(flux_layout::WidthPolicy::ViewportFraction(*fraction))
        }
        flux_config::Width::Preset { .. } => Err("default width preset is recursive"),
    }
}
fn width_value(width: flux_layout::WidthPolicy) -> serde_json::Value {
    match width {
        flux_layout::WidthPolicy::Fixed(logical_px) => serde_json::json!({"logical_px":logical_px}),
        flux_layout::WidthPolicy::ViewportFraction(fraction) => {
            serde_json::json!({"fraction":fraction})
        }
    }
}

fn run_worker(
    shared: Arc<Shared>,
    mut coordinator: ConfigCoordinator,
    keymap: String,
    ping: calloop::ping::Ping,
) {
    let context = xkb::Context::new(xkb::CONTEXT_NO_ENVIRONMENT_NAMES);
    let map = xkb::Keymap::new_from_string(
        &context,
        keymap,
        xkb::KEYMAP_FORMAT_TEXT_V1,
        xkb::KEYMAP_COMPILE_NO_FLAGS,
    );
    let mut active = None;
    loop {
        let (job, retired, ready) = {
            let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
            while !c.closed && c.pending.is_none() && c.retired.is_none() && !c.evaluation_ready {
                c = shared.changed.wait(c).unwrap_or_else(|e| e.into_inner());
            }
            if c.closed {
                break;
            }
            let values = (c.pending.take(), c.retired.take(), c.evaluation_ready);
            c.evaluation_ready = false;
            values
        };
        let retired_any = retired.is_some();
        drop(retired);
        if retired_any {
            ping.ping();
        }
        if let Some((generation, path)) = job {
            // Move stale completions out before dropping them off dispatch.
            let old = shared
                .control
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .completion
                .take();
            drop(old);
            match coordinator.request_path(path) {
                Ok(inner) => active = Some((generation, inner)),
                Err(_) => publish(
                    &shared,
                    generation,
                    Err("configuration request could not be admitted"),
                    &ping,
                ),
            }
        }
        if ready
            && let Some(result) = coordinator.take_completion()
            && let Some((generation, inner)) = active
            && inner == result.generation
        {
            let result = result
                .result
                .map_err(|failure| match failure.kind {
                    flux_config::ConfigFailureKind::Io => {
                        "configuration source or helper I/O failed"
                    }
                    flux_config::ConfigFailureKind::Invalid => {
                        "configuration schema or source validation failed"
                    }
                    flux_config::ConfigFailureKind::Timeout => "configuration evaluation timed out",
                    flux_config::ConfigFailureKind::Helper => {
                        "configuration helper rejected evaluation"
                    }
                    flux_config::ConfigFailureKind::Cancelled => {
                        "configuration evaluation cancelled"
                    }
                })
                .and_then(|candidate| {
                    let config = &candidate.config().config;
                    if config.layout.gaps > 1024.0 {
                        return Err("layout.gaps exceeds the nested model limit of 1024");
                    }
                    if config.bindings.iter().any(|b| {
                        matches!(
                            b.action,
                            flux_core::Action::Fullscreen { .. }
                                | flux_core::Action::Floating { .. }
                                | flux_core::Action::Overview { .. }
                                | flux_core::Action::Lock
                        )
                    }) {
                        return Err("a binding action is unavailable in the nested backend");
                    }
                    let bindings = BindingSet::compile(
                        &config.bindings,
                        map.as_ref()
                            .ok_or("seat keymap could not be reconstructed")?,
                        UnmappedPolicy::Reject,
                    )
                    .map_err(|_| "configuration bindings failed seat-keymap validation")?;
                    Ok(Prepared {
                        default_width: resolve_default_width(&config.layout)?,
                        desired: config.clone(),
                        bindings,
                    })
                });
            publish(&shared, generation, result, &ping);
        }
    }
    // No join or large source/schema/table destruction runs during dispatch.
    let _ = coordinator.shutdown();
    let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
    c.pending = None;
    c.completion = None;
    c.retired = None;
}
fn publish(
    shared: &Shared,
    generation: u64,
    value: Result<Prepared, Failure>,
    ping: &calloop::ping::Ping,
) {
    let old = {
        let mut c = shared.control.lock().unwrap_or_else(|e| e.into_inner());
        if c.closed || c.latest != generation {
            return;
        }
        c.completion.replace((generation, value))
    };
    drop(old);
    ping.ping();
}

impl crate::Fluxwm {
    pub fn init_config(
        &mut self,
        handle: &calloop::LoopHandle<'static, Self>,
        source: Option<PathBuf>,
        helper: PathBuf,
    ) -> std::io::Result<()> {
        let Some(source) = source else {
            return Ok(());
        };
        let (ping, events) = calloop::ping::make_ping()?;
        handle
            .insert_source(events, |(), _, _| {})
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        let keyboard = self.seat.get_keyboard().expect("startup seat has keyboard");
        let keymap = keyboard.with_xkb_state(self, |context| {
            let xkb = context.xkb().lock().unwrap();
            // SAFETY: string serialization borrows only under this Xkb lock;
            // the worker receives owned bytes and creates its own native map.
            unsafe { xkb.keymap() }.get_as_string(xkb::KEYMAP_FORMAT_TEXT_V1)
        });
        let mut config = RuntimeConfig::new(source, helper, keymap, ping)?;
        config.request().map_err(std::io::Error::other)?;
        self.config = Some(config);
        Ok(())
    }
    pub fn request_config_reload(&mut self) -> Result<u64, Failure> {
        if self.stopping || self.shutdown_deadline.is_some() {
            return Err("instance is shutting down");
        }
        self.config
            .as_mut()
            .ok_or("configuration loading is disabled")?
            .request()
    }
    pub fn drive_config(&mut self) {
        if self.stopping
            || self.shutdown_deadline.is_some()
            || self
                .seat
                .get_keyboard()
                .is_some_and(|k| !k.pressed_keys().is_empty() || k.is_grabbed())
            || self.seat.get_pointer().is_some_and(|p| p.is_grabbed())
            || !self.pointer_presses.is_empty()
        {
            return;
        }
        let Some(mut config) = self.config.take() else {
            return;
        };
        if let Some((generation, result)) = config.completion() {
            match result {
                Err(error) => {
                    config.status = "failed";
                    config.last_error = Some(error);
                    self.bump_sequence();
                }
                Ok(prepared) => {
                    let mut layout = self.layout.clone();
                    if layout
                        .reconfigure_with_default_width(
                            prepared.desired.layout.gaps,
                            prepared.desired.layout.mode,
                            prepared.default_width,
                        )
                        .is_err()
                    {
                        config.status = "failed";
                        config.last_error =
                            Some("configuration layout would invalidate live geometry");
                        config.retire(Retired {
                            _desired: Some(prepared.desired),
                            _bindings: Some(prepared.bindings),
                            _layout: Some(layout),
                        });
                    } else if self.sequence == u64::MAX {
                        config.status = "failed";
                        config.last_error = Some("state sequence exhausted");
                        config.retire(Retired {
                            _desired: Some(prepared.desired),
                            _bindings: Some(prepared.bindings),
                            _layout: Some(layout),
                        });
                    } else {
                        let old = Retired {
                            _desired: config.desired.replace(prepared.desired),
                            _bindings: Some(std::mem::replace(
                                &mut self.bindings,
                                prepared.bindings,
                            )),
                            _layout: Some(std::mem::replace(&mut self.layout, layout)),
                        };
                        config.applied = Some(generation);
                        config.status = "applied";
                        config.last_error = None;
                        config.retire(old);
                        self.relayout();
                        self.restore_keyboard_focus();
                    }
                    self.bump_sequence();
                }
            }
        }
        self.config = Some(config);
    }
    pub fn effective_config(&self) -> serde_json::Value {
        let config = self.config.as_ref();
        serde_json::json!({
            "generation":config.and_then(|c|c.applied).map(|g|g.to_string()),
            "desired":config.and_then(|c|c.desired.as_ref()).map(|c|serde_json::json!({"layout":{"gaps":c.layout.gaps,"mode":c.layout.mode,"default_width":c.layout.default_width,"width_presets":c.layout.width_presets},"binding_entries":c.bindings.len()})),
            "effective":{"layout":{"gaps":self.layout.gap(),"mode":self.layout.mode(),"default_width":width_value(self.layout.default_width())},"binding_entries":self.bindings.len(),"keymap":"startup-seat-keymap"},
            "unsupported":["layout.outer_gap","layout.focus_margin","layout.canvas","input","outputs","animations","decoration","rules","autostart","security"],
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn configured_width_resolves_one_based_presets_without_recursive_defaults() {
        use flux_config::Width;
        use flux_layout::WidthPolicy;
        let mut layout = flux_config::Layout::default();
        assert_eq!(
            resolve_default_width(&layout).unwrap(),
            WidthPolicy::ViewportFraction(0.5)
        );
        layout.width_presets = vec![
            Width::Pixels { logical_px: 400. },
            Width::Fraction { fraction: 0.25 },
        ];
        layout.default_width = Width::Preset { preset: 1 };
        assert_eq!(
            resolve_default_width(&layout).unwrap(),
            WidthPolicy::Fixed(400.)
        );
        layout.default_width = Width::Preset { preset: 2 };
        assert_eq!(
            resolve_default_width(&layout).unwrap(),
            WidthPolicy::ViewportFraction(0.25)
        );
        for preset in [0, 3, usize::MAX] {
            layout.default_width = Width::Preset { preset };
            assert!(resolve_default_width(&layout).is_err());
        }
        layout.default_width = Width::Preset { preset: 1 };
        layout.width_presets[0] = Width::Preset { preset: 1 };
        assert!(resolve_default_width(&layout).is_err());
        layout.default_width = Width::Pixels { logical_px: 640. };
        assert_eq!(
            resolve_default_width(&layout).unwrap(),
            WidthPolicy::Fixed(640.)
        );
    }
    #[test]
    fn worker_publication_rejects_stale_and_closed_generations() {
        let shared = Shared::default();
        let (ping, _source) = calloop::ping::make_ping().unwrap();
        shared.control.lock().unwrap().latest = 2;
        publish(&shared, 1, Err("old evaluation"), &ping);
        assert!(shared.control.lock().unwrap().completion.is_none());
        publish(&shared, 2, Err("current evaluation"), &ping);
        assert_eq!(
            shared.control.lock().unwrap().completion.take().unwrap().0,
            2
        );
        shared.control.lock().unwrap().closed = true;
        publish(&shared, 2, Err("late evaluation"), &ping);
        assert!(shared.control.lock().unwrap().completion.is_none());
    }
    #[test]
    fn accepted_request_hides_prepared_generation_before_worker_observes_request() {
        let shared = Arc::new(Shared::default());
        {
            let mut c = shared.control.lock().unwrap();
            c.latest = 1;
            c.completion = Some((
                1,
                Ok(Prepared {
                    desired: Config::default(),
                    bindings: BindingSet::default(),
                    default_width: flux_layout::WidthPolicy::ViewportFraction(0.5),
                }),
            ));
        }
        // Deliberately park the owned worker until shutdown, making the exact
        // request-before-worker-observation interval deterministic.
        let observed = shared.clone();
        let worker = std::thread::spawn(move || {
            let mut c = observed.control.lock().unwrap();
            while !c.closed {
                c = observed.changed.wait(c).unwrap();
            }
        });
        let mut config = RuntimeConfig {
            source: "/unused".into(),
            shared,
            worker: Some(worker),
            desired: None,
            applied: None,
            status: "pending",
            last_error: None,
        };
        assert_eq!(config.request().unwrap(), 2);
        assert!(config.diagnostics()["prepared_generation"].is_null());
        assert!(config.completion().is_none());
        {
            let c = config.shared.control.lock().unwrap();
            assert_eq!(c.pending.as_ref().unwrap().0, 2);
            assert_eq!(c.completion.as_ref().unwrap().0, 1);
        }
        config.shutdown().unwrap();
    }

    #[test]
    fn termination_seals_ready_config_publication() {
        const GUARD: &str = "FLUX_TEST_CONFIG_STOP_CHILD";
        if std::env::var_os(GUARD).as_deref() != Some(std::ffi::OsStr::new("1")) {
            let dir = tempfile::Builder::new()
                .prefix("fc-stop-")
                .tempdir()
                .unwrap();
            let log_path = dir.path().join("child.log");
            let output = std::fs::File::create(&log_path).unwrap();
            let mut child = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "config::tests::termination_seals_ready_config_publication",
                    "--nocapture",
                ])
                .env(GUARD, "1")
                .env("XDG_RUNTIME_DIR", dir.path())
                .stdout(output.try_clone().unwrap())
                .stderr(output)
                .spawn()
                .unwrap();
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            let status = loop {
                if let Some(status) = child.try_wait().unwrap() {
                    break status;
                }
                if std::time::Instant::now() >= deadline {
                    let _ = child.kill();
                    let _ = child.wait();
                    panic!("owned config-stop child exceeded deadline");
                }
                std::thread::sleep(std::time::Duration::from_millis(5));
            };
            assert!(
                status.success(),
                "{}",
                std::fs::read_to_string(log_path).unwrap()
            );
            return;
        }
        let mut event_loop = calloop::EventLoop::try_new().unwrap();
        let display = smithay::reexports::wayland_server::Display::new().unwrap();
        let mut state = crate::Fluxwm::new(&mut event_loop, display, "stop-test").unwrap();
        let keyboard = state.seat.get_keyboard().unwrap();
        let desired = Config::default();
        let bindings = keyboard.with_xkb_state(&mut state, |context| {
            let xkb = context.xkb().lock().unwrap();
            // SAFETY: compilation borrows this map only under the native lock.
            BindingSet::compile(
                &desired.bindings,
                unsafe { xkb.keymap() },
                UnmappedPolicy::Reject,
            )
            .unwrap()
        });
        let shared = Arc::new(Shared::default());
        {
            let mut c = shared.control.lock().unwrap();
            c.latest = 1;
            c.completion = Some((
                1,
                Ok(Prepared {
                    desired,
                    bindings,
                    default_width: flux_layout::WidthPolicy::ViewportFraction(0.5),
                }),
            ));
        }
        // A live owned test worker avoids the stopped-worker fallback masking
        // removal of the actual publication gate. It does not evaluate Lua.
        let worker_shared = shared.clone();
        let worker = std::thread::spawn(move || {
            let mut c = worker_shared.control.lock().unwrap();
            while !c.closed {
                c = worker_shared.changed.wait(c).unwrap();
            }
        });
        state.config = Some(RuntimeConfig {
            source: "/unused".into(),
            shared,
            worker: Some(worker),
            desired: None,
            applied: None,
            status: "pending",
            last_error: None,
        });
        let before = state.effective_config();
        let before_sequence = state.sequence;
        assert!(
            !state
                .config
                .as_ref()
                .unwrap()
                .worker
                .as_ref()
                .unwrap()
                .is_finished()
        );
        state.stop(); // same sealing method as SIGTERM, timer, host-close and fail
        state.finish_dispatch();
        state.finish_dispatch(); // main's final dispatch cannot publish either
        assert_eq!(state.effective_config(), before);
        assert_eq!(state.config.as_ref().unwrap().applied, None);
        assert_eq!(state.sequence, before_sequence);
        assert!(state.request_config_reload().is_err());
        assert!(
            state
                .apply_action(flux_core::Action::WorkspaceMode {
                    mode: flux_core::LayoutMode::Canvas
                })
                .is_err()
        );
        state.config.as_mut().unwrap().shutdown().unwrap();
    }
}
