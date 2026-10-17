use crate::{CompiledConfig, Config, ConfigError, Result, invalid};
#[cfg(unix)]
use std::os::{fd::AsRawFd, unix::process::CommandExt};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

pub const MAX_SOURCE_BYTES: usize = 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_DIAGNOSTIC_BYTES: usize = 16 * 1024;
#[derive(Debug, Clone, Copy)]
pub struct HelperLimits {
    pub deadline: Duration,
    pub address_space_bytes: u64,
}
impl Default for HelperLimits {
    fn default() -> Self {
        Self {
            deadline: Duration::from_secs(1),
            address_space_bytes: 256 * 1024 * 1024,
        }
    }
}
struct Reap(Child);
impl Drop for Reap {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
#[cfg(unix)]
fn nonblocking(fd: std::os::fd::RawFd) -> std::io::Result<()> {
    // SAFETY: fcntl acts on an owned pipe descriptor; flags are checked for errors.
    unsafe {
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags < 0 || libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) < 0 {
            return Err(std::io::Error::last_os_error());
        }
    }
    Ok(())
}
fn drain(reader: &mut impl Read, bytes: &mut Vec<u8>, bound: usize, label: &str) -> Result<bool> {
    let mut buf = [0; 8192];
    // Bound work per pass too: a continually writing child cannot starve the deadline.
    for _ in 0..16 {
        match reader.read(&mut buf) {
            Ok(0) => return Ok(true),
            Ok(n) => {
                if bytes.len() + n > bound {
                    return Err(invalid(format!(
                        "helper {label} exceeds {bound} byte limit"
                    )));
                }
                bytes.extend_from_slice(&buf[..n]);
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Ok(false),
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e.into()),
        }
    }
    Ok(false)
}
/// Evaluate a bounded source snapshot in a fresh exec-based helper, kill/reap on any
/// failure, validate the normalized result again, and compile regexes in Rust.
/// This is blocking worker-thread work. ConfigCoordinator supplies bounded asynchronous ownership.
#[cfg(target_os = "linux")]
pub fn load_with_helper(
    helper: &Path,
    source: &[u8],
    limits: HelperLimits,
) -> Result<CompiledConfig> {
    load_with_helper_cancellable(
        helper,
        source,
        limits,
        &std::sync::atomic::AtomicBool::new(false),
        |_| {},
    )
}

#[cfg(target_os = "linux")]
pub(crate) fn load_with_helper_cancellable(
    helper: &Path,
    source: &[u8],
    limits: HelperLimits,
    cancelled: &std::sync::atomic::AtomicBool,
    on_spawn: impl FnOnce(u32),
) -> Result<CompiledConfig> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(ConfigError::Cancelled);
    }
    if source.len() > MAX_SOURCE_BYTES {
        return Err(invalid("config source exceeds 1 MiB"));
    }
    std::str::from_utf8(source).map_err(|_| invalid("config source must be UTF-8 text"))?;
    if limits.deadline.is_zero() || limits.address_space_bytes < 64 * 1024 * 1024 {
        return Err(invalid(
            "helper limits require a nonzero deadline and at least 64 MiB address space",
        ));
    }
    let start = Instant::now();
    let mut command = Command::new(helper);
    command
        .arg("--evaluate")
        .env_clear()
        .env("LANG", "C.UTF-8")
        .current_dir("/")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let address_space = limits.address_space_bytes;
    // SAFETY: pre_exec only invokes async-signal-safe syscalls. close_range with
    // CLOEXEC explicitly removes every non-stdio fd at exec, including unknown
    // inherited descriptors, without breaking Command's exec-error pipe.
    unsafe {
        command.pre_exec(move || {
            if libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) != 0 {
                return Err(std::io::Error::last_os_error());
            }
            if libc::syscall(
                libc::SYS_close_range,
                3u32,
                u32::MAX,
                libc::CLOSE_RANGE_CLOEXEC,
            ) != 0
            {
                return Err(std::io::Error::last_os_error());
            }
            for (resource, value) in [
                (libc::RLIMIT_CORE, 0),
                (libc::RLIMIT_FSIZE, 0),
                (libc::RLIMIT_CPU, 2),
                (libc::RLIMIT_NOFILE, 32),
                (libc::RLIMIT_AS, address_space),
            ] {
                let r = libc::rlimit {
                    rlim_cur: value as libc::rlim_t,
                    rlim_max: value as libc::rlim_t,
                };
                if libc::setrlimit(resource, &r) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
            }
            Ok(())
        });
    }
    let mut child = Reap(command.spawn()?);
    on_spawn(child.0.id());
    let mut stdin = child.0.stdin.take();
    let mut stdout = child.0.stdout.take().expect("piped stdout");
    let mut stderr = child.0.stderr.take().expect("piped stderr");
    nonblocking(stdin.as_ref().unwrap().as_raw_fd())?;
    nonblocking(stdout.as_raw_fd())?;
    nonblocking(stderr.as_raw_fd())?;
    let mut written = 0;
    let mut result = Vec::new();
    let mut diagnostic = Vec::new();
    loop {
        if start.elapsed() >= limits.deadline {
            return Err(ConfigError::Timeout);
        }
        if cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(ConfigError::Cancelled);
        }
        if let Some(input) = stdin.as_mut() {
            match input.write(&source[written..]) {
                Ok(n) => written += n,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) => {}
                Err(e) if e.kind() == std::io::ErrorKind::BrokenPipe => {
                    stdin.take();
                }
                Err(e) => return Err(e.into()),
            }
            if written == source.len() {
                stdin.take();
            }
        }
        let out_done = drain(&mut stdout, &mut result, MAX_RESULT_BYTES, "result")?;
        let err_done = drain(
            &mut stderr,
            &mut diagnostic,
            MAX_DIAGNOSTIC_BYTES,
            "diagnostic",
        )?;
        if let Some(status) = child.0.try_wait()? {
            if !out_done || !err_done {
                continue;
            }
            if !status.success() {
                return Err(ConfigError::Helper {
                    status: status.to_string(),
                    diagnostic: String::from_utf8_lossy(&diagnostic).into_owned(),
                });
            }
            return Config::from_json(&result)?.validate();
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}
#[cfg(not(target_os = "linux"))]
pub fn load_with_helper(
    _helper: &Path,
    _source: &[u8],
    _limits: HelperLimits,
) -> Result<CompiledConfig> {
    Err(invalid(
        "configuration helper containment is currently qualified only for Linux",
    ))
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn load_with_helper_cancellable(
    helper: &Path,
    source: &[u8],
    limits: HelperLimits,
    _cancelled: &std::sync::atomic::AtomicBool,
    _on_spawn: impl FnOnce(u32),
) -> Result<CompiledConfig> {
    load_with_helper(helper, source, limits)
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::*;
    #[test]
    fn error_cleanup_kills_and_reaps_the_owned_child() {
        let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
        let pid = child.id();
        drop(Reap(child));
        let mut status = 0;
        // SAFETY: waitpid with WNOHANG cannot block. ECHILD proves no zombie is
        // retained even if unrelated processes later reuse the numerical PID.
        let result = unsafe { libc::waitpid(pid as i32, &mut status, libc::WNOHANG) };
        assert_eq!(result, -1);
        assert_eq!(
            std::io::Error::last_os_error().raw_os_error(),
            Some(libc::ECHILD)
        );
    }
}
