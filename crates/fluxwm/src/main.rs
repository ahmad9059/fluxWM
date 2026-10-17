// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
#![allow(irrefutable_let_patterns)]
mod actions;
mod grabs;
mod handlers;
mod input;
mod ipc;
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
    exit_after: Option<Duration>,
    ready: Option<PathBuf>,
    stats: Option<PathBuf>,
    screenshot: Option<PathBuf>,
    command: Vec<OsString>,
}

fn options() -> Result<Option<Options>, Box<dyn std::error::Error>> {
    let mut result = Options::default();
    let mut args = std::env::args_os().skip(1);
    while let Some(arg) = args.next() {
        match arg.to_str() {
            Some("--help" | "-h") => {
                println!(
                    "fluxwm --backend nested [--socket NAME] [--exit-after-ms N] [--ready-file PATH] [--stats-file PATH] [--screenshot NEW.ppm] [-- COMMAND ARG...]\nOnly the nested backend is currently implemented. Output files must not already exist."
                );
                return Ok(None);
            }
            Some("--backend") => {
                if args.next().as_deref() != Some(std::ffi::OsStr::new("nested")) {
                    return Err("only --backend nested is implemented".into());
                }
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

pub(crate) struct SupervisedChild(pub Child);
impl Drop for SupervisedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
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
            state.loop_signal.stop();
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
    state.screenshot = options.screenshot;
    winit::init_winit(&mut event_loop, &mut state)?;
    if let Some(duration) = options.exit_after {
        event_loop
            .handle()
            .insert_source(Timer::from_duration(duration), |_, _, state| {
                state.loop_signal.stop();
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
            && let Err(error) = write_json_new(&path, &serde_json::json!({"backend":"nested", "wayland_display":state.socket_name.to_string_lossy(), "pid":std::process::id(), "status":"dispatch-ready", "ipc_socket":state.ipc_socket})) {
                state.fail(format!("readiness publication: {error}"));
            }
    })?;
    state.finish_dispatch();
    // Join and release process ownership outside event dispatch. Dropping the
    // event loop drains any delivered outcomes still held by its channel.
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
