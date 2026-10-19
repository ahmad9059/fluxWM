// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
#![allow(irrefutable_let_patterns)]
mod actions;
mod autostart;
mod config;
mod configure;
#[cfg(feature = "native")]
mod cursor;
mod grabs;
mod handlers;
mod health;
mod input;
mod ipc;
#[cfg(feature = "native")]
mod native;
mod scene;
mod state;
mod winit;

use smithay::reexports::{
    calloop::{
        EventLoop,
        timer::{TimeoutAction, Timer},
    },
    wayland_server::Display,
};
pub(crate) use state::Fluxwm;
use std::{
    ffi::OsString,
    path::PathBuf,
    process::{Child, Command},
    time::Duration,
};

#[derive(Default)]
struct Options {
    socket: Option<String>,
    backend: Option<String>,
    native_output: Option<(u64, u32)>,
    config: Option<PathBuf>,
    no_config: bool,
    config_helper: Option<PathBuf>,
    exit_after: Option<Duration>,
    ready: Option<PathBuf>,
    stats: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    screenshot_after: Option<Duration>,
    command: Vec<OsString>,
}

fn options() -> Result<Option<Options>, Box<dyn std::error::Error>> {
    let mut result = Options::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "fluxwm --backend nested [--config PATH | --no-config] [--config-helper PATH] [--socket NAME] [--exit-after-ms N] [--ready-file PATH] [--stats-file PATH] [--screenshot NEW.ppm] [--screenshot-after-ms N] [-- COMMAND ARG...]\nNative builds also accept --backend native --native-output DEVICE:CONNECTOR. Native screenshots are unsupported. Output files must not already exist."
                );
                return Ok(None);
            }
            Some("--backend") => {
                if result.backend.is_some() {
                    return Err("--backend must appear once".into());
                }
                let backend = args
                    .next()
                    .ok_or("missing backend")?
                    .into_string()
                    .map_err(|_| "backend must be UTF-8")?;
                if backend != "nested" && backend != "native" {
                    return Err("backend must be nested or native".into());
                }
                result.backend = Some(backend);
            }
            Some("--native-output") => {
                if result.native_output.is_some() {
                    return Err("--native-output must appear once".into());
                }
                let value = args
                    .next()
                    .ok_or("missing native output DEVICE:CONNECTOR")?
                    .into_string()
                    .map_err(|_| "native output must be UTF-8")?;
                result.native_output = Some(parse_native_output(&value)?);
            }
            Some("--config") => {
                if result.config.is_some() || result.no_config {
                    return Err(
                        "--config and --no-config are mutually exclusive and must appear once"
                            .into(),
                    );
                }
                result.config = Some(args.next().ok_or("missing config path")?.into());
            }
            Some("--no-config") => {
                if result.config.is_some() || result.no_config {
                    return Err(
                        "--config and --no-config are mutually exclusive and must appear once"
                            .into(),
                    );
                }
                result.no_config = true;
            }
            Some("--config-helper") => {
                if result.config_helper.is_some() {
                    return Err("--config-helper must appear once".into());
                }
                result.config_helper =
                    Some(args.next().ok_or("missing config helper path")?.into());
            }
            Some("--socket") => {
                result.socket = Some(
                    args.next()
                        .ok_or("missing socket name")?
                        .into_string()
                        .map_err(|_| "socket must be UTF-8")?,
                )
            }
            Some("--exit-after-ms") => {
                let value: u64 = args
                    .next()
                    .ok_or("missing timeout")?
                    .to_str()
                    .ok_or("timeout must be UTF-8")?
                    .parse()?;
                if !(1..=86_400_000).contains(&value) {
                    return Err("timeout must be 1..86400000 milliseconds".into());
                }
                result.exit_after = Some(Duration::from_millis(value));
            }
            Some("--screenshot-after-ms") => {
                let value: u64 = args
                    .next()
                    .ok_or("missing screenshot delay")?
                    .to_str()
                    .ok_or("delay must be UTF-8")?
                    .parse()?;
                if value > 86_400_000 {
                    return Err("screenshot delay must be 0..86400000 milliseconds".into());
                }
                result.screenshot_after = Some(Duration::from_millis(value));
            }
            Some("--ready-file") => {
                result.ready = Some(args.next().ok_or("missing ready path")?.into())
            }
            Some("--stats-file") => {
                result.stats = Some(args.next().ok_or("missing stats path")?.into())
            }
            Some("--screenshot") => {
                result.screenshot = Some(args.next().ok_or("missing screenshot path")?.into())
            }
            Some("--") => {
                result.command.extend(args);
                break;
            }
            _ => return Err(format!("unknown option {arg:?}; use --help").into()),
        }
    }
    if result.screenshot_after.is_some() && result.screenshot.is_none() {
        return Err("--screenshot-after-ms requires --screenshot".into());
    }
    validate_backend_options(&result)?;
    let socket = result
        .socket
        .get_or_insert_with(|| format!("fluxwm-{}", std::process::id()));
    if socket.is_empty()
        || socket.len() > 64
        || !socket
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
    {
        return Err("socket name must be 1..64 ASCII letters, digits, '-' or '_'".into());
    }
    Ok(Some(result))
}

fn parse_native_output(value: &str) -> Result<(u64, u32), Box<dyn std::error::Error>> {
    let (device, connector) = value
        .split_once(':')
        .ok_or("native output must be DEVICE:CONNECTOR decimal IDs")?;
    if device.is_empty()
        || connector.is_empty()
        || !device
            .bytes()
            .chain(connector.bytes())
            .all(|c| c.is_ascii_digit())
    {
        return Err("native output must contain decimal device and connector IDs".into());
    }
    let device = device.parse()?;
    let connector = connector.parse()?;
    if connector == 0 {
        return Err("connector ID must be nonzero".into());
    }
    Ok((device, connector))
}
fn validate_backend_options(options: &Options) -> Result<(), Box<dyn std::error::Error>> {
    if options.backend.as_deref() == Some("native") {
        if !cfg!(feature = "native") {
            return Err("native backend requires a build with --features native".into());
        }
        if options.native_output.is_none() {
            return Err("native backend requires --native-output DEVICE:CONNECTOR".into());
        }
        if options.screenshot.is_some() || options.screenshot_after.is_some() {
            return Err("screenshots are not supported by the native backend".into());
        }
    } else if options.native_output.is_some() {
        return Err("--native-output requires --backend native".into());
    }
    Ok(())
}

fn absolute_config_path(path: PathBuf) -> std::io::Result<PathBuf> {
    use std::os::unix::ffi::OsStrExt;
    let valid = |p: &std::path::Path| {
        let bytes = p.as_os_str().as_bytes();
        !bytes.is_empty()
            && bytes.len() <= flux_config::MAX_SOURCE_PATH_BYTES
            && !bytes.contains(&0)
    };
    if !valid(&path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "configuration paths must contain 1..4096 NUL-free bytes",
        ));
    }
    let path = std::path::absolute(path)?;
    if !valid(&path) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "absolute configuration path exceeds 4096 bytes",
        ));
    }
    Ok(path)
}
fn config_source(
    explicit: Option<PathBuf>,
    disabled: bool,
    xdg: Option<OsString>,
    home: Option<OsString>,
) -> std::io::Result<Option<PathBuf>> {
    if disabled {
        return Ok(None);
    }
    explicit
        .or_else(|| {
            xdg.filter(|v| !v.is_empty())
                .map(PathBuf::from)
                .filter(|p| p.is_absolute())
                .or_else(|| {
                    home.map(|p| PathBuf::from(p).join(".config"))
                        .filter(|p| p.is_absolute())
                })
                .map(|p| p.join("fluxwm/config.lua"))
        })
        .map(absolute_config_path)
        .transpose()
}

pub(crate) struct SupervisedChild(pub Child);
impl Drop for SupervisedChild {
    fn drop(&mut self) {
        match self.0.try_wait() {
            Ok(Some(_)) => return,
            Err(error) if error.raw_os_error() == Some(libc::ECHILD) => return,
            _ => (),
        }
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn write_json_new(
    path: &std::path::Path,
    value: &impl serde::Serialize,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer_pretty(&mut file, value)?;
    file.write_all(b"\n")?;
    file.flush()?;
    // Publish a complete file atomically without replacing an existing path.
    file.persist_noclobber(path)?;
    Ok(())
}

pub(crate) fn spawn_client(
    command: &std::ffi::OsStr,
    args: &[OsString],
    socket: &std::ffi::OsStr,
    ipc: Option<&std::path::Path>,
) -> std::io::Result<Child> {
    use std::os::unix::process::CommandExt;
    let mut command = Command::new(command);
    command
        .args(args)
        .env("WAYLAND_DISPLAY", socket)
        .env_remove("WAYLAND_SOCKET")
        .env_remove("DISPLAY");
    if let Some(path) = ipc {
        command.env("FLUXWM_SOCKET", path);
    } else {
        command.env_remove("FLUXWM_SOCKET");
    }
    // SAFETY: child-only closure uses async-signal-safe signal-mask operations,
    // with no allocator/locks. Undo only masks installed by this executable.
    unsafe {
        command.pre_exec(|| {
            let mut signals: libc::sigset_t = std::mem::zeroed();
            libc::sigemptyset(&mut signals);
            libc::sigaddset(&mut signals, libc::SIGTERM);
            libc::sigaddset(&mut signals, libc::SIGINT);
            libc::sigaddset(&mut signals, libc::SIGCHLD);
            let error = libc::pthread_sigmask(libc::SIG_UNBLOCK, &signals, std::ptr::null_mut());
            if error == 0 {
                Ok(())
            } else {
                Err(std::io::Error::from_raw_os_error(error))
            }
        });
    }
    command.spawn()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "fluxwm=info".into()),
        )
        .init();
    let Some(options) = options()? else {
        return Ok(());
    };
    for path in [&options.ready, &options.stats, &options.screenshot]
        .into_iter()
        .flatten()
    {
        if path.try_exists()? {
            return Err(format!("output already exists: {}", path.display()).into());
        }
    }
    // Validate runtime directory without changing host process or service environment.
    use std::os::unix::fs::MetadataExt;
    let runtime =
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").ok_or("XDG_RUNTIME_DIR is required")?);
    let metadata = std::fs::symlink_metadata(&runtime)?;
    if !metadata.is_dir()
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
    {
        return Err("XDG_RUNTIME_DIR must be a real same-user directory with mode 0700".into());
    }
    // Retain an independent dispatcher owner, declared before state, so
    // dropping the loop cannot unmask termination signals before owned children
    // and sockets finish cleanup (including early error returns).
    let signals = calloop::signals::Signals::new(&[
        calloop::signals::Signal::SIGTERM,
        calloop::signals::Signal::SIGINT,
        calloop::signals::Signal::SIGCHLD,
    ])?;
    let signal_dispatcher = calloop::Dispatcher::new(signals, |event, _, state: &mut Fluxwm| {
        if event.signal() != calloop::signals::Signal::SIGCHLD {
            state.stop();
        } else if let Some(worker) = state.autostart.as_ref() {
            worker.notify_child_exit();
        }
    });
    let mut event_loop: EventLoop<Fluxwm> = EventLoop::try_new()?;
    let display: Display<Fluxwm> = Display::new()?;
    event_loop
        .handle()
        .register_dispatcher(signal_dispatcher.clone())
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let mut state = Fluxwm::new(
        &mut event_loop,
        display,
        options.socket.as_deref().ok_or("missing socket")?,
    )?;
    if let Some(delay) = options.screenshot_after {
        let mut path = options.screenshot;
        event_loop
            .handle()
            .insert_source(Timer::from_duration(delay), move |_, _, state| {
                state.screenshot = path.take();
                state.request_redraw();
                TimeoutAction::Drop
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    } else {
        state.screenshot = options.screenshot;
    }
    #[cfg(feature = "native")]
    let mut native_handle = if options.backend.as_deref() == Some("native") {
        let (device, connector) = options.native_output.ok_or("missing native output")?;
        Some(native::init(
            &mut event_loop,
            &mut state,
            device,
            connector,
        )?)
    } else {
        winit::init_winit(&mut event_loop, &mut state)?;
        None
    };
    #[cfg(not(feature = "native"))]
    winit::init_winit(&mut event_loop, &mut state)?;
    if let Some(duration) = options.exit_after {
        event_loop
            .handle()
            .insert_source(Timer::from_duration(duration), |_, _, state| {
                state.stop();
                TimeoutAction::Drop
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
    }
    state.init_ipc(
        &event_loop.handle(),
        &runtime,
        options.socket.as_deref().ok_or("missing instance")?,
    )?;
    state.init_processes(&event_loop.handle())?;
    let source = config_source(
        options.config,
        options.no_config,
        std::env::var_os("XDG_CONFIG_HOME"),
        std::env::var_os("HOME"),
    )?;
    let helper = match options.config_helper {
        Some(path) => absolute_config_path(path)?,
        None => absolute_config_path(std::env::current_exe()?.with_file_name("fluxwm-config"))?,
    };
    if source.is_some() {
        state.init_autostart(&event_loop.handle())?;
    }
    state.init_config(&event_loop.handle(), source, helper)?;
    let mut child = if let Some((command, args)) = options.command.split_first() {
        Some(SupervisedChild(spawn_client(
            command,
            args,
            &state.socket_name,
            state.ipc_socket.as_deref(),
        )?))
    } else {
        None
    };
    let mut ready = options.ready;
    event_loop.run(None, &mut state, |state| {
        state.finish_dispatch();
        if let Some(child) = child.as_mut()
            && let Err(error) = child.0.try_wait() {
                state.fail(format!("startup child reaping: {error}"));
            }
        if let Some(path) = ready.take()
            && let Err(error) = write_json_new(&path, &serde_json::json!({"backend":state.backend, "wayland_display":state.socket_name.to_string_lossy(), "pid":std::process::id(), "status":"dispatch-ready", "ipc_socket":state.ipc_socket})) {
                state.fail(format!("readiness publication: {error}"));
            }
    })?;
    state.finish_dispatch();
    #[cfg(feature = "native")]
    if let Some(handle) = native_handle.as_mut() {
        handle.shutdown(&mut state);
    }
    // Join and release process ownership outside event dispatch. Dropping the
    // event loop drains any delivered outcomes still held by its channel.
    state.shutdown_autostart()?;
    if let Some(mut config) = state.config.take() {
        config.shutdown()?;
    }
    if let Some(worker) = state.spawn_worker.take() {
        worker.shutdown()?;
    }
    drop(event_loop);
    state.spawned.clear();
    state.stats.final_windows = state.windows.len();
    if let Some(writer) = state.screenshot_writer.take() {
        writer.join().map_err(|_| "screenshot worker panicked")??;
    }
    if let Some(path) = options.stats {
        write_json_new(&path, &state.stats)?;
    }
    if let Some(child) = child.as_mut()
        && let Some(status) = child.0.try_wait()?
        && !status.success()
    {
        return Err(format!("supervised command failed: {status}").into());
    }
    if let Some(error) = state.fatal_error {
        return Err(error.into());
    }
    Ok(())
}

#[cfg(test)]
mod option_tests {
    use super::*;
    #[test]
    fn native_output_ids_are_explicit_and_bounded() {
        assert_eq!(parse_native_output("0:1").unwrap(), (0, 1));
        assert_eq!(
            parse_native_output("18446744073709551615:4294967295").unwrap(),
            (u64::MAX, u32::MAX)
        );
        for value in [
            "",
            "1",
            "1:0",
            "1:2:3",
            "-1:2",
            "+1:2",
            "1: 2",
            "1:4294967296",
            "18446744073709551616:1",
        ] {
            assert!(parse_native_output(value).is_err(), "{value}");
        }
    }
    #[test]
    fn native_options_reject_ambiguous_or_unsupported_modes() {
        let mut options = Options {
            native_output: Some((0, 1)),
            ..Options::default()
        };
        assert!(validate_backend_options(&options).is_err());
        options.backend = Some("native".into());
        assert_eq!(
            validate_backend_options(&options).is_ok(),
            cfg!(feature = "native")
        );
        options.screenshot = Some("frame.ppm".into());
        assert!(validate_backend_options(&options).is_err());
        options.screenshot = None;
        options.native_output = None;
        assert!(validate_backend_options(&options).is_err());
    }
    #[test]
    fn source_discovery_is_explicit_and_ignores_relative_xdg_roots() {
        let root = Some(OsString::from("/config-root"));
        let home = Some(OsString::from("/user-home"));
        assert_eq!(
            config_source(None, false, root.clone(), home.clone())
                .unwrap()
                .unwrap(),
            PathBuf::from("/config-root/fluxwm/config.lua")
        );
        for xdg in [
            None,
            Some(OsString::new()),
            Some(OsString::from("relative")),
        ] {
            assert_eq!(
                config_source(None, false, xdg, home.clone())
                    .unwrap()
                    .unwrap(),
                PathBuf::from("/user-home/.config/fluxwm/config.lua")
            );
        }
        assert_eq!(config_source(None, false, None, None).unwrap(), None);
        assert_eq!(
            config_source(None, true, root.clone(), home.clone()).unwrap(),
            None
        );
        assert_eq!(
            config_source(Some("/explicit.lua".into()), false, root, home)
                .unwrap()
                .unwrap(),
            PathBuf::from("/explicit.lua")
        );
    }
    #[test]
    fn configuration_paths_reject_empty_nul_and_expansion_beyond_budget() {
        use std::os::unix::ffi::OsStringExt;
        for path in [
            PathBuf::new(),
            PathBuf::from(OsString::from_vec(b"/config\0suffix".to_vec())),
            PathBuf::from("x".repeat(4096)),
        ] {
            assert_eq!(
                absolute_config_path(path).unwrap_err().kind(),
                std::io::ErrorKind::InvalidInput
            );
        }
        assert!(
            absolute_config_path("relative.lua".into())
                .unwrap()
                .is_absolute()
        );
    }
}
