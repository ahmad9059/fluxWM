//! Bounded local IPC. Call `Server::poll` from the compositor event loop.
#[cfg(feature = "calloop")]
mod driver;
#[cfg(feature = "calloop")]
pub use driver::EventLoopDriver;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::HashSet,
    fs,
    io::{self, Read, Write},
    os::{
        fd::{AsRawFd, FromRawFd, RawFd},
        unix::{
            ffi::OsStrExt,
            fs::{MetadataExt, PermissionsExt},
            net::{UnixListener, UnixStream},
        },
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

pub const VERSION: u32 = 1;
pub const MAX_PAYLOAD: usize = 1_048_576;
pub const MAX_DEPTH: usize = 32;
pub const MAX_CONNECTIONS: usize = 32;
const MAX_QUEUE: usize = MAX_PAYLOAD + 4;
const IO_BUDGET: usize = 65536;
const DEADLINE: Duration = Duration::from_secs(5);
const IDLE_DEADLINE: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Sequence(pub u64);
impl From<Sequence> for String {
    fn from(v: Sequence) -> String {
        v.0.to_string()
    }
}
impl TryFrom<String> for Sequence {
    type Error = &'static str;
    fn try_from(v: String) -> Result<Self, Self::Error> {
        if v.is_empty()
            || v.len() > 20
            || !v.bytes().all(|b| b.is_ascii_digit())
            || (v.len() > 1 && v.starts_with('0'))
        {
            return Err("expected canonical u64 decimal string");
        }
        v.parse().map(Self).map_err(|_| "counter overflow")
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum Method {
    #[serde(rename = "version")]
    Version,
    #[serde(rename = "capabilities")]
    Capabilities,
    #[serde(rename = "windows.list")]
    Windows,
    #[serde(rename = "workspaces.list")]
    Workspaces,
    #[serde(rename = "outputs.list")]
    Outputs,
    #[serde(rename = "config.effective")]
    EffectiveConfig,
    #[serde(rename = "diagnostics")]
    Diagnostics,
    #[serde(rename = "config.check")]
    ConfigCheck,
    #[serde(rename = "config.reload")]
    ConfigReload,
    #[serde(rename = "action")]
    Action,
    #[serde(other)]
    Unknown,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Request {
    pub version: u32,
    pub id: String,
    pub method: Method,
    #[serde(default = "empty_object")]
    pub params: Value,
}
fn empty_object() -> Value {
    serde_json::json!({})
}
impl Request {
    pub fn new(id: impl Into<String>, method: Method) -> Self {
        Self {
            version: VERSION,
            id: id.into(),
            method,
            params: empty_object(),
        }
    }
    pub fn action(&self) -> Result<flux_core::Action, ErrorBody> {
        serde_json::from_value(self.params.clone())
            .map_err(|e| ErrorBody::new(ErrorCode::InvalidParams, e.to_string()))
    }
    pub fn validate(&self) -> Result<(), ErrorBody> {
        if self.version != VERSION {
            return Err(ErrorBody::new(
                ErrorCode::UnsupportedVersion,
                "only IPC version 1 is supported",
            ));
        }
        if self.id.is_empty() || self.id.len() > 128 {
            return Err(ErrorBody::new(
                ErrorCode::InvalidParams,
                "request ID must contain 1..128 UTF-8 bytes",
            ));
        }
        if self.method == Method::Unknown {
            return Err(ErrorBody::new(
                ErrorCode::UnknownMethod,
                "unknown IPC method",
            ));
        }
        if !self.params.is_object() {
            return Err(ErrorBody::new(
                ErrorCode::InvalidParams,
                "params must be an object",
            ));
        }
        if self.method == Method::Action {
            self.action()?;
        } else if self.method == Method::ConfigCheck {
            let obj = self.params.as_object().unwrap();
            if obj.keys().any(|k| k != "path")
                || obj.get("path").is_some_and(|v| {
                    !v.is_string()
                        || v.as_str().unwrap().is_empty()
                        || v.as_str().unwrap().len() > 4096
                        || v.as_str().unwrap().contains('\0')
                })
            {
                return Err(ErrorBody::new(
                    ErrorCode::InvalidParams,
                    "config.check accepts only an optional nonempty, NUL-free path of at most 4096 bytes",
                ));
            }
        } else if !self.params.as_object().unwrap().is_empty() {
            return Err(ErrorBody::new(
                ErrorCode::InvalidParams,
                "method accepts no params",
            ));
        }
        Ok(())
    }
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorCode {
    UnknownMethod,
    UnsupportedVersion,
    InvalidParams,
    StaleObject,
    PermissionDenied,
    Busy,
    DeviceOperationFailed,
    NotReady,
    InternalError,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ErrorBody {
    pub code: ErrorCode,
    pub message: String,
}
impl ErrorBody {
    pub fn new(code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Outcome {
    Success { result: Value },
    Failure { error: ErrorBody },
}
#[derive(Debug, Clone, Serialize)]
pub struct Response {
    pub version: u32,
    pub id: String,
    pub state_sequence: Sequence,
    #[serde(flatten)]
    pub outcome: Outcome,
}
impl<'de> Deserialize<'de> for Response {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        struct Wire {
            version: u32,
            id: String,
            state_sequence: Sequence,
            #[serde(flatten)]
            fields: serde_json::Map<String, Value>,
        }
        let mut wire = Wire::deserialize(d)?;
        if wire.fields.len() != 1 {
            return Err(serde::de::Error::custom(
                "response requires exactly one result or error",
            ));
        }
        let outcome = if let Some(result) = wire.fields.remove("result") {
            Outcome::Success { result }
        } else if let Some(error) = wire.fields.remove("error") {
            Outcome::Failure {
                error: serde_json::from_value(error).map_err(serde::de::Error::custom)?,
            }
        } else {
            return Err(serde::de::Error::custom("unknown response field"));
        };
        Ok(Self {
            version: wire.version,
            id: wire.id,
            state_sequence: wire.state_sequence,
            outcome,
        })
    }
}
impl Response {
    pub fn success(id: impl Into<String>, sequence: u64, result: Value) -> Self {
        Self {
            version: VERSION,
            id: id.into(),
            state_sequence: Sequence(sequence),
            outcome: Outcome::Success { result },
        }
    }
    pub fn error(id: impl Into<String>, sequence: u64, error: ErrorBody) -> Self {
        Self {
            version: VERSION,
            id: id.into(),
            state_sequence: Sequence(sequence),
            outcome: Outcome::Failure { error },
        }
    }
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Event {
    pub version: u32,
    pub event: String,
    pub state_sequence: Sequence,
    pub config_generation: Sequence,
    pub payload: Value,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Window {
    pub id: flux_core::WindowId,
    pub workspace: flux_core::WorkspaceId,
    pub title: String,
    pub app_id: String,
    pub focused: bool,
    pub geometry: flux_core::Rect,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    pub id: flux_core::WorkspaceId,
    pub output: Option<flux_core::OutputId>,
    pub mode: flux_core::LayoutMode,
    pub focused: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Output {
    pub id: flux_core::OutputId,
    pub name: String,
    pub geometry: flux_core::Rect,
    pub scale: f64,
}

// A seed preserves depth across arrays/maps and rejects keys before values can overwrite.
struct Seed(usize);
impl<'de> serde::de::DeserializeSeed<'de> for Seed {
    type Value = Value;
    fn deserialize<D: serde::Deserializer<'de>>(self, d: D) -> Result<Value, D::Error> {
        if self.0 > MAX_DEPTH {
            return Err(serde::de::Error::custom("JSON nesting limit exceeded"));
        }
        d.deserialize_any(self)
    }
}
impl<'de> serde::de::Visitor<'de> for Seed {
    type Value = Value;
    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("bounded JSON")
    }
    fn visit_bool<E: serde::de::Error>(self, v: bool) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_i64<E: serde::de::Error>(self, v: i64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_u64<E: serde::de::Error>(self, v: u64) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_f64<E: serde::de::Error>(self, v: f64) -> Result<Value, E> {
        serde_json::Number::from_f64(v)
            .map(Value::Number)
            .ok_or_else(|| E::custom("nonfinite JSON number"))
    }
    fn visit_str<E: serde::de::Error>(self, v: &str) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_string<E: serde::de::Error>(self, v: String) -> Result<Value, E> {
        Ok(v.into())
    }
    fn visit_unit<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_none<E: serde::de::Error>(self) -> Result<Value, E> {
        Ok(Value::Null)
    }
    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut v = Vec::new();
        while let Some(x) = a.next_element_seed(Seed(self.0 + 1))? {
            v.push(x);
        }
        Ok(Value::Array(v))
    }
    fn visit_map<A: serde::de::MapAccess<'de>>(self, mut a: A) -> Result<Value, A::Error> {
        let mut v = serde_json::Map::new();
        while let Some(k) = a.next_key::<String>()? {
            if v.contains_key(&k) {
                return Err(serde::de::Error::custom("duplicate JSON key"));
            }
            v.insert(k, a.next_value_seed(Seed(self.0 + 1))?);
        }
        Ok(Value::Object(v))
    }
}
pub fn decode<T: serde::de::DeserializeOwned>(payload: &[u8]) -> io::Result<T> {
    use serde::de::DeserializeSeed;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD {
        return Err(invalid("invalid frame length"));
    }
    let mut d = serde_json::Deserializer::from_slice(payload);
    let value = Seed(0)
        .deserialize(&mut d)
        .map_err(|e| invalid(e.to_string()))?;
    d.end().map_err(|e| invalid(e.to_string()))?;
    serde_json::from_value(value).map_err(|e| invalid(e.to_string()))
}
fn invalid(s: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, s.into())
}
/// Enforces payload bytes and value depth while serde emits JSON, before recursion
/// can walk arbitrarily deep values or allocate an oversized encoded buffer.
struct BoundedJson {
    bytes: Vec<u8>,
    containers: Vec<(bool, bool)>, // object, expecting key
    in_string: bool,
    escaped: bool,
}
impl Write for BoundedJson {
    fn write(&mut self, input: &[u8]) -> io::Result<usize> {
        if input.len() > MAX_PAYLOAD.saturating_sub(self.bytes.len()) {
            return Err(invalid("encoded payload exceeds limit"));
        }
        for &byte in input {
            if self.in_string {
                if self.escaped {
                    self.escaped = false;
                } else if byte == b'\\' {
                    self.escaped = true;
                } else if byte == b'"' {
                    self.in_string = false;
                }
                continue;
            }
            let is_key = self
                .containers
                .last()
                .is_some_and(|(object, key)| *object && *key);
            match byte {
                b'"' => {
                    if !is_key && self.containers.len() > MAX_DEPTH {
                        return Err(invalid("JSON nesting limit exceeded"));
                    }
                    self.in_string = true;
                }
                b'[' | b'{' => {
                    if self.containers.len() > MAX_DEPTH {
                        return Err(invalid("JSON nesting limit exceeded"));
                    }
                    self.containers.push((byte == b'{', byte == b'{'));
                }
                b']' | b'}' => {
                    self.containers.pop();
                }
                b':' => {
                    if let Some((_, key)) = self.containers.last_mut() {
                        *key = false;
                    }
                }
                b',' => {
                    if let Some((object, key)) = self.containers.last_mut() {
                        *key = *object;
                    }
                }
                b'n' | b't' | b'f' | b'-' | b'0'..=b'9' if self.containers.len() > MAX_DEPTH => {
                    return Err(invalid("JSON nesting limit exceeded"));
                }
                _ => {}
            }
        }
        self.bytes.extend_from_slice(input);
        Ok(input.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let mut writer = BoundedJson {
        bytes: Vec::new(),
        containers: Vec::new(),
        in_string: false,
        escaped: false,
    };
    serde_json::to_writer(&mut writer, value).map_err(|e| invalid(e.to_string()))?;
    // Validate arbitrary custom Serialize implementations too (including duplicate keys).
    let _: Value = decode(&writer.bytes)?;
    let mut framed = Vec::with_capacity(writer.bytes.len() + 4);
    framed.extend_from_slice(&(writer.bytes.len() as u32).to_be_bytes());
    framed.extend(writer.bytes);
    Ok(framed)
}
#[derive(Default)]
pub struct Decoder {
    buffer: Vec<u8>,
}
impl Decoder {
    pub fn push(&mut self, bytes: &[u8]) -> io::Result<()> {
        if self.buffer.len() + bytes.len() > MAX_PAYLOAD + 4 + IO_BUDGET {
            return Err(invalid("receive buffer limit"));
        }
        self.buffer.extend_from_slice(bytes);
        self.check_length()?;
        Ok(())
    }
    fn check_length(&self) -> io::Result<()> {
        if self.buffer.len() >= 4 {
            let n = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
            if n == 0 || n > MAX_PAYLOAD {
                return Err(invalid("invalid frame length"));
            }
        }
        Ok(())
    }
    pub fn next_frame(&mut self) -> io::Result<Option<Vec<u8>>> {
        self.check_length()?;
        if self.buffer.len() < 4 {
            return Ok(None);
        }
        let n = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
        if self.buffer.len() < 4 + n {
            return Ok(None);
        }
        let tail = self.buffer.split_off(4 + n);
        let frame = self.buffer[4..].to_vec();
        self.buffer = tail;
        Ok(Some(frame))
    }
    pub fn has_complete_frame(&self) -> bool {
        if self.buffer.len() < 4 {
            return false;
        }
        let length = u32::from_be_bytes(self.buffer[..4].try_into().unwrap()) as usize;
        length == 0 || length > MAX_PAYLOAD || self.buffer.len() >= length + 4
    }
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

struct Pending {
    bytes: Vec<u8>,
    offset: usize,
    id: String,
    enqueued: Instant,
}
struct Connection {
    token: u64,
    stream: UnixStream,
    decoder: Decoder,
    out: Option<Pending>,
    ids: HashSet<String>,
    partial_since: Option<Instant>,
    last_activity: Instant,
    eof: bool,
    close_after_flush: bool,
    io_continue: bool,
}
/// Identity is separate from raw fd: OS descriptor numbers can be reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Endpoint {
    Listener,
    Client(u64),
}
#[derive(Debug, Clone, Copy)]
pub struct IoInterest {
    pub endpoint: Endpoint,
    pub fd: RawFd,
    pub readable: bool,
    pub writable: bool,
}
#[derive(Debug)]
pub struct Readiness {
    pub interests: Vec<IoInterest>,
    /// Dispatch without waiting for another fd edge: complete requests are buffered.
    pub needs_dispatch: bool,
    /// Arm an absolute monotonic timer even when no fd is ready.
    pub next_deadline: Option<Instant>,
}
#[derive(Debug, Default, Clone, Copy)]
pub struct ServerStats {
    pub accepted: u64,
    pub rejected_capacity: u64,
    pub rejected_credentials: u64,
    pub accept_errors: u64,
    pub disconnected: u64,
    pub protocol_errors: u64,
    pub timed_out: u64,
}
struct SocketCleanup {
    path: PathBuf,
    identity: Option<(u64, u64)>,
}
impl Drop for SocketCleanup {
    fn drop(&mut self) {
        if let Ok(meta) = fs::symlink_metadata(&self.path)
            && self.identity == Some((meta.dev(), meta.ino()))
        {
            let _ = fs::remove_file(&self.path);
        }
    }
}
pub struct Server {
    listener: UnixListener,
    socket: SocketCleanup,
    connections: Vec<Connection>,
    state_sequence: u64,
    next_token: u64,
    accept_retry_at: Option<Instant>,
    stats: ServerStats,
}
impl Server {
    /// Directory ownership and permissions implement the documented same-UID boundary.
    pub fn bind(runtime_dir: &Path, instance: &str) -> io::Result<Self> {
        Self::bind_with_setup(runtime_dir, instance, |_| Ok(()))
    }
    fn bind_with_setup(
        runtime_dir: &Path,
        instance: &str,
        setup: impl FnOnce(&UnixListener) -> io::Result<()>,
    ) -> io::Result<Self> {
        if instance.is_empty()
            || instance.len() > 64
            || !instance
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
        {
            return Err(invalid("invalid instance name"));
        }
        check_directory(runtime_dir)?;
        let dir = runtime_dir.join("fluxwm");
        match std::os::unix::fs::DirBuilderExt::mode(&mut fs::DirBuilder::new(), 0o700).create(&dir)
        {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
        check_directory(&dir)?;
        let path = dir.join(format!("{instance}.sock"));
        let listener = UnixListener::bind(&path)?;
        // Capture path identity before every fallible setup operation. No existing path
        // is removed on bind failure; cleanup never unlinks a replacement socket.
        let mut socket = SocketCleanup {
            path,
            identity: None,
        };
        let meta = fs::symlink_metadata(&socket.path)?;
        socket.identity = Some((meta.dev(), meta.ino()));
        fs::set_permissions(&socket.path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        setup(&listener)?;
        Ok(Self {
            listener,
            socket,
            connections: Vec::new(),
            state_sequence: 0,
            next_token: 1,
            accept_retry_at: None,
            stats: ServerStats::default(),
        })
    }
    pub fn socket_path(&self) -> &Path {
        &self.socket.path
    }
    /// Current core sequence at entry. Each handler response must contain the exact
    /// resulting sequence, which becomes authoritative for later transport errors.
    pub fn set_state_sequence(&mut self, sequence: u64) {
        self.state_sequence = sequence;
    }
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
    pub fn stats(&self) -> ServerStats {
        self.stats
    }
    /// Snapshot registration interests after every poll. Reconcile by Endpoint,
    /// never raw fd alone; drop removed registrations before adding reused fds.
    pub fn readiness(&self, now: Instant) -> Readiness {
        let mut interests = Vec::with_capacity(self.connections.len() + 1);
        let mut next_deadline = self.accept_retry_at;
        if self.accept_retry_at.is_none_or(|at| now >= at) {
            interests.push(IoInterest {
                endpoint: Endpoint::Listener,
                fd: self.listener.as_raw_fd(),
                readable: true,
                writable: false,
            });
        }
        let mut needs_dispatch = false;
        for c in &self.connections {
            let complete = c.decoder.has_complete_frame();
            needs_dispatch |= c.io_continue;
            needs_dispatch |= c.out.is_none() && (complete || c.eof || c.close_after_flush);
            let readable = !c.eof && !c.close_after_flush && c.out.is_none() && !complete;
            if readable || c.out.is_some() {
                interests.push(IoInterest {
                    endpoint: Endpoint::Client(c.token),
                    fd: c.stream.as_raw_fd(),
                    readable,
                    writable: c.out.is_some(),
                });
            }
            for deadline in [
                Some(c.last_activity + IDLE_DEADLINE),
                c.partial_since.map(|t| t + DEADLINE),
                c.out.as_ref().map(|p| p.enqueued + DEADLINE),
            ]
            .into_iter()
            .flatten()
            {
                next_deadline = Some(next_deadline.map_or(deadline, |old| old.min(deadline)));
            }
        }
        Readiness {
            interests,
            needs_dispatch,
            next_deadline,
        }
    }
    /// Nonblocking, bounded IO and at most 16 handler calls per client. Handler
    /// must snapshot or enqueue commands without blocking and report current sequence.
    pub fn poll(&mut self, now: Instant, mut handler: impl FnMut(Request) -> Response) {
        if self.accept_retry_at.is_none_or(|at| now >= at) {
            self.accept_retry_at = None;
            for _ in 0..MAX_CONNECTIONS {
                match self.listener.accept() {
                    Ok((stream, _)) => {
                        if self.connections.len() >= MAX_CONNECTIONS {
                            self.stats.rejected_capacity += 1;
                            self.accept_retry_at = Some(now + Duration::from_millis(100));
                            break;
                        }
                        if !same_uid(&stream) {
                            self.stats.rejected_credentials += 1;
                            continue;
                        }
                        if stream.set_nonblocking(true).is_err() {
                            self.stats.accept_errors += 1;
                            continue;
                        }
                        let Some(next) = self.next_token.checked_add(1) else {
                            self.stats.accept_errors += 1;
                            break;
                        };
                        self.connections.push(Connection {
                            token: self.next_token,
                            stream,
                            decoder: Decoder::default(),
                            out: None,
                            ids: HashSet::new(),
                            partial_since: None,
                            last_activity: now,
                            eof: false,
                            close_after_flush: false,
                            io_continue: false,
                        });
                        self.next_token = next;
                        self.stats.accepted += 1;
                    }
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => {
                        self.stats.accept_errors += 1;
                        self.accept_retry_at = Some(now + Duration::from_millis(100));
                        break;
                    }
                }
            }
        }
        self.connections.retain_mut(|c| {
            match process(
                c,
                now,
                &mut self.state_sequence,
                &mut handler,
                &mut self.stats,
            ) {
                Ok(()) => true,
                Err(e) => {
                    self.stats.disconnected += 1;
                    if e.kind() == io::ErrorKind::TimedOut {
                        self.stats.timed_out += 1;
                    }
                    false
                }
            }
        });
    }
}
fn check_directory(path: &Path) -> io::Result<()> {
    let m = fs::symlink_metadata(path)?;
    if !m.is_dir() || m.uid() != unsafe { libc::geteuid() } || m.mode() & 0o077 != 0 {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "runtime directory must be a real, same-user directory with mode 0700",
        ));
    }
    Ok(())
}
fn same_uid(stream: &UnixStream) -> bool {
    let mut cred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let r = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut cred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    r == 0
        && len as usize == std::mem::size_of::<libc::ucred>()
        && cred.uid == unsafe { libc::geteuid() }
}
fn timeout() -> io::Error {
    io::Error::new(io::ErrorKind::TimedOut, "IPC deadline exceeded")
}
fn flush(c: &mut Connection, budget: &mut usize) -> io::Result<()> {
    for attempt in 0..32 {
        let Some(front) = c.out.as_mut() else {
            break;
        };
        if attempt == 31 {
            c.io_continue = true;
        }
        if *budget == 0 {
            c.io_continue = true;
            break;
        }
        let end = (front.offset + *budget).min(front.bytes.len());
        match c.stream.write(&front.bytes[front.offset..end]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => {
                front.offset += n;
                *budget -= n;
                if front.offset == front.bytes.len() {
                    let id = c.out.take().unwrap().id;
                    c.ids.remove(&id);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
fn process(
    c: &mut Connection,
    now: Instant,
    state_sequence: &mut u64,
    handler: &mut impl FnMut(Request) -> Response,
    stats: &mut ServerStats,
) -> io::Result<()> {
    if now.saturating_duration_since(c.last_activity) >= IDLE_DEADLINE
        || c.partial_since
            .is_some_and(|t| now.saturating_duration_since(t) >= DEADLINE)
        || c.out
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.enqueued) >= DEADLINE)
    {
        return Err(timeout());
    }
    c.io_continue = false;
    let mut write_budget = IO_BUDGET;
    flush(c, &mut write_budget)?;
    // Reserve space for one maximum-sized response before consuming another request.
    // A slow writer therefore cannot make command side effects outrun response capacity.
    if c.out.is_some() {
        return Ok(());
    }
    if !c.eof && !c.close_after_flush && !c.decoder.has_complete_frame() {
        let mut buffer = [0u8; 8192];
        let mut read = 0;
        for attempt in 0..32 {
            if attempt == 31 {
                c.io_continue = true;
            }
            if read >= IO_BUDGET {
                c.io_continue = true;
                break;
            }
            match c.stream.read(&mut buffer) {
                Ok(0) => {
                    c.eof = true;
                    break;
                }
                Ok(n) => {
                    read += n;
                    c.last_activity = now;
                    if c.decoder.is_empty() {
                        c.partial_since = Some(now);
                    }
                    if c.decoder.push(&buffer[..n]).is_err() {
                        stats.protocol_errors += 1;
                        c.close_after_flush = true;
                        break;
                    }
                    if c.decoder.has_complete_frame() {
                        break;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(e),
            }
        }
    }
    for _ in 0..16 {
        if c.close_after_flush || write_budget == 0 || c.out.is_some() {
            break;
        }
        let frame = match c.decoder.next_frame() {
            Ok(Some(frame)) => frame,
            Ok(None) => break,
            Err(_) => {
                stats.protocol_errors += 1;
                c.close_after_flush = true;
                break;
            }
        };
        // Every consumed frame gets its own partial-frame interval. Complete buffered
        // frames are scheduled immediately through readiness().needs_dispatch.
        c.partial_since = (!c.decoder.is_empty()).then_some(now);
        let request: Request = match decode(&frame) {
            Ok(request) => request,
            Err(_) => {
                stats.protocol_errors += 1;
                c.close_after_flush = true;
                break;
            }
        };
        if request.id.is_empty() || request.id.len() > 128 || !c.ids.insert(request.id.clone()) {
            stats.protocol_errors += 1;
            c.close_after_flush = true;
            break;
        }
        let id = request.id.clone();
        let response = match request.validate() {
            Ok(()) => handler(request),
            Err(error) => Response::error(&id, *state_sequence, error),
        };
        if response.id != id
            || response.version != VERSION
            || response.state_sequence.0 < *state_sequence
        {
            return Err(invalid(
                "handler returned inconsistent identity/version/state sequence",
            ));
        }
        *state_sequence = response.state_sequence.0;
        let bytes = match encode(&response) {
            Ok(bytes) => bytes,
            Err(_) => encode(&Response::error(
                &id,
                *state_sequence,
                ErrorBody::new(
                    ErrorCode::InternalError,
                    "response exceeds IPC shape or size limits; action outcome may require a fresh snapshot",
                ),
            ))?,
        };
        debug_assert!(bytes.len() <= MAX_QUEUE);
        c.out = Some(Pending {
            bytes,
            offset: 0,
            id,
            enqueued: now,
        });
        flush(c, &mut write_budget)?;
    }
    if c.decoder.is_empty() {
        c.partial_since = None;
    }
    if c.eof && !c.decoder.is_empty() && !c.decoder.has_complete_frame() {
        stats.protocol_errors += 1;
        c.close_after_flush = true;
    }
    if c.out.is_none() && (c.close_after_flush || (c.eof && c.decoder.is_empty())) {
        return Err(io::ErrorKind::ConnectionAborted.into());
    }
    Ok(())
}
fn wait_fd(fd: RawFd, events: i16, deadline: Instant) -> io::Result<()> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or_else(timeout)?;
        if remaining.is_zero() {
            return Err(timeout());
        }
        let millis = remaining
            .as_millis()
            .saturating_add(1)
            .min(i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        let result = unsafe { libc::poll(&mut descriptor, 1, millis) };
        if result > 0 {
            if descriptor.revents & libc::POLLNVAL != 0 {
                return Err(io::ErrorKind::InvalidInput.into());
            }
            return Ok(());
        }
        if result == 0 {
            return Err(timeout());
        }
        let error = io::Error::last_os_error();
        if error.kind() != io::ErrorKind::Interrupted {
            return Err(error);
        }
    }
}
fn connect_until(path: &Path, deadline: Instant) -> io::Result<UnixStream> {
    let bytes = path.as_os_str().as_bytes();
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.is_empty() || bytes.contains(&0) || bytes.len() >= address.sun_path.len() {
        return Err(invalid("invalid Unix socket pathname"));
    }
    address.sun_family = libc::AF_UNIX as libc::sa_family_t;
    for (to, from) in address.sun_path.iter_mut().zip(bytes) {
        *to = *from as libc::c_char;
    }
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let stream = unsafe { UnixStream::from_raw_fd(raw) };
    let length =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    loop {
        if Instant::now() >= deadline {
            return Err(timeout());
        }
        let result =
            unsafe { libc::connect(raw, &address as *const _ as *const libc::sockaddr, length) };
        if result == 0 {
            return Ok(stream);
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EISCONN) => return Ok(stream),
            Some(libc::EINTR) => continue,
            Some(libc::EAGAIN) => {
                // AF_UNIX backlog-full is a failed attempt, not a pending connect.
                // Retry at a bounded interval rather than spinning on writable fd.
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(remaining.min(Duration::from_millis(5)));
            }
            Some(libc::EINPROGRESS) | Some(libc::EALREADY) => {
                wait_fd(raw, libc::POLLOUT, deadline)?;
                if let Some(error) = stream.take_error()? {
                    return Err(error);
                }
                return Ok(stream);
            }
            _ => return Err(error),
        }
    }
}
fn transfer_until(
    stream: &mut UnixStream,
    bytes: &mut [u8],
    write: bool,
    deadline: Instant,
) -> io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err(timeout());
        }
        let result = if write {
            stream.write(&bytes[offset..])
        } else {
            stream.read(&mut bytes[offset..])
        };
        match result {
            Ok(0) => {
                return Err(if write {
                    io::ErrorKind::WriteZero
                } else {
                    io::ErrorKind::UnexpectedEof
                }
                .into());
            }
            Ok(count) => offset += count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => wait_fd(
                stream.as_raw_fd(),
                if write { libc::POLLOUT } else { libc::POLLIN },
                deadline,
            )?,
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
/// CLI/worker helper. One absolute wall deadline covers encoding, nonblocking
/// connect, all request bytes, response prefix and payload. Never use in render loop.
pub fn call(path: &Path, request: &Request) -> io::Result<Response> {
    call_with_timeout(path, request, DEADLINE)
}
pub fn call_with_timeout(
    path: &Path,
    request: &Request,
    duration: Duration,
) -> io::Result<Response> {
    let deadline = Instant::now()
        .checked_add(duration)
        .ok_or_else(|| invalid("deadline overflow"))?;
    request.validate().map_err(|e| invalid(e.message))?;
    let mut frame = encode(request)?;
    let mut stream = connect_until(path, deadline)?;
    if !same_uid(&stream) {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "IPC server UID differs from client UID",
        ));
    }
    transfer_until(&mut stream, &mut frame, true, deadline)?;
    let mut prefix = [0; 4];
    transfer_until(&mut stream, &mut prefix, false, deadline)?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size == 0 || size > MAX_PAYLOAD {
        return Err(invalid("invalid response length"));
    }
    let mut payload = vec![0; size];
    transfer_until(&mut stream, &mut payload, false, deadline)?;
    let response: Response = decode(&payload)?;
    if Instant::now() >= deadline {
        return Err(timeout());
    }
    if response.version != VERSION || response.id != request.id {
        return Err(invalid("response version or ID mismatch"));
    }
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn strict_json() {
        for raw in [
            r#"{"version":1,"id":"a","id":"b","method":"version"}"#,
            r#"{"version":1,"id":"a","method":"version","params":{"a":1,"a":2}}"#,
        ] {
            assert!(decode::<Request>(raw.as_bytes()).is_err());
        }
        let nested = format!(
            "{}0{}",
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert!(decode::<Value>(nested.as_bytes()).is_err());
        assert!(decode::<Value>(b"{} {}").is_err());
        assert!(decode::<Value>(&[0xff]).is_err());
    }
    #[test]
    fn frame_fragmentation_and_coalescing() {
        let req = Request::new("a", Method::Version);
        let frame = encode(&req).unwrap();
        let mut decoder = Decoder::default();
        for byte in &frame[..frame.len() - 1] {
            decoder.push(&[*byte]).unwrap();
            assert!(decoder.next_frame().unwrap().is_none());
        }
        decoder.push(&frame[frame.len() - 1..]).unwrap();
        let parsed: Request = decode(&decoder.next_frame().unwrap().unwrap()).unwrap();
        assert_eq!(parsed.id, "a");
        assert!(decoder.is_empty());
        let mut both = frame.clone();
        both.extend(&frame);
        decoder.push(&both).unwrap();
        assert!(decoder.next_frame().unwrap().is_some());
        assert!(decoder.next_frame().unwrap().is_some());
    }
    #[test]
    fn oversized_prefix_rejected_before_body() {
        for size in [0, MAX_PAYLOAD as u32 + 1, u32::MAX] {
            let mut d = Decoder::default();
            assert!(d.push(&size.to_be_bytes()).is_err());
        }
    }
    #[test]
    fn stale_version_ids_params_and_counters() {
        let mut r = Request::new("r", Method::Version);
        r.version = 0;
        assert_eq!(
            r.validate().unwrap_err().code,
            ErrorCode::UnsupportedVersion
        );
        r.version = 1;
        r.id = "x".repeat(129);
        assert!(r.validate().is_err());
        r.id = "x".into();
        r.params = serde_json::json!({"surprise":true});
        assert!(r.validate().is_err());
        assert!(serde_json::from_str::<Sequence>("42").is_err());
        assert!(serde_json::from_str::<Sequence>("\"18446744073709551616\"").is_err());
        assert_eq!(
            serde_json::to_string(&Sequence(u64::MAX)).unwrap(),
            "\"18446744073709551615\""
        );
    }
    #[test]
    fn response_roundtrip() {
        for r in [
            Response::success("r", u64::MAX, serde_json::json!({"ok":true})),
            Response::error("r", 3, ErrorBody::new(ErrorCode::Busy, "busy")),
        ] {
            let encoded = encode(&r).unwrap();
            let decoded: Response = decode(&encoded[4..]).unwrap();
            assert_eq!(decoded.id, "r");
        }
    }
    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let p = std::env::temp_dir().join(format!(
                "flux-ipc-test-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            ));
            std::os::unix::fs::DirBuilderExt::mode(&mut fs::DirBuilder::new(), 0o700)
                .create(&p)
                .unwrap();
            Self(p)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[test]
    fn server_permissions_collision_and_cleanup() {
        let temp = Temp::new();
        let server = Server::bind(&temp.0, "test").unwrap();
        let socket = server.socket_path().to_owned();
        assert_eq!(fs::metadata(&socket).unwrap().mode() & 0o777, 0o600);
        assert!(Server::bind(&temp.0, "test").is_err());
        drop(server);
        assert!(!socket.exists());
        fs::set_permissions(&temp.0, fs::Permissions::from_mode(0o755)).unwrap();
        assert!(Server::bind(&temp.0, "bad").is_err());
    }
    #[test]
    fn server_fragmentation_deadline_duplicate_and_stale_version() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "test").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let now = Instant::now();
        let frame = encode(&Request::new("a", Method::Version)).unwrap();
        client.write_all(&frame[..2]).unwrap();
        server.poll(now, |_| panic!("partial frame dispatched"));
        assert_eq!(server.connection_count(), 1);
        server.poll(
            now + DEADLINE + Duration::from_millis(1),
            |_| unreachable!(),
        );
        assert_eq!(server.connection_count(), 0);
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        let mut req = Request::new("b", Method::Version);
        req.version = 2;
        client.write_all(&encode(&req).unwrap()).unwrap();
        server.poll(now, |_| panic!("unsupported version dispatched"));
        let mut prefix = [0; 4];
        client.read_exact(&mut prefix).unwrap();
        let mut payload = vec![0; u32::from_be_bytes(prefix) as usize];
        client.read_exact(&mut payload).unwrap();
        let response: Response = decode(&payload).unwrap();
        assert!(matches!(
            response.outcome,
            Outcome::Failure {
                error: ErrorBody {
                    code: ErrorCode::UnsupportedVersion,
                    ..
                }
            }
        ));
        drop(client);
        server.poll(now, |_| unreachable!());
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        let mut duplicate = frame.clone();
        duplicate.extend(&frame);
        client.write_all(&duplicate).unwrap();
        let mut calls = 0;
        server.poll(now, |r| {
            calls += 1;
            Response::success(r.id, 0, Value::Null)
        });
        // The first response flushes before consuming another request, so ID reuse
        // is legal here: only one request can be outstanding under backpressure.
        assert_eq!(calls, 2);
        assert_eq!(server.connection_count(), 1);
    }
    #[test]
    fn slow_reader_backpressure_and_per_message_deadline() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "slow").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        for id in ["1", "2", "3"] {
            client
                .write_all(&encode(&Request::new(id, Method::Version)).unwrap())
                .unwrap();
        }
        let now = Instant::now();
        let mut dispatched = 0;
        server.poll(now, |r| {
            dispatched += 1;
            Response::success(r.id, 0, Value::String("x".repeat(MAX_PAYLOAD / 2)))
        });
        assert_eq!(dispatched, 1);
        assert_eq!(server.connection_count(), 1);
        let interest = server
            .readiness(now)
            .interests
            .into_iter()
            .find(|i| matches!(i.endpoint, Endpoint::Client(_)))
            .unwrap();
        assert!(interest.writable);
        assert!(!interest.readable);
        server.poll(now + DEADLINE, |_| {
            panic!("backpressure must prevent further dispatch")
        });
        assert_eq!(server.connection_count(), 0);
        assert_eq!(server.stats().timed_out, 1);
    }
    #[test]
    fn bounded_connections_and_nonblocking_idle() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "idle").unwrap();
        let mut clients = Vec::new();
        for _ in 0..MAX_CONNECTIONS + 1 {
            clients.push(UnixStream::connect(server.socket_path()).unwrap());
            server.poll(Instant::now(), |_| unreachable!());
        }
        assert_eq!(server.connection_count(), MAX_CONNECTIONS);
    }
    fn receive(client: &mut UnixStream) -> Response {
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut prefix = [0; 4];
        client.read_exact(&mut prefix).unwrap();
        let mut bytes = vec![0; u32::from_be_bytes(prefix) as usize];
        client.read_exact(&mut bytes).unwrap();
        decode(&bytes).unwrap()
    }
    #[test]
    fn complete_half_closed_requests_flush_and_truncated_tail_does_not_erase_them() {
        use std::net::Shutdown;
        for truncated_tail in [false, true] {
            let temp = Temp::new();
            let mut server = Server::bind(&temp.0, "eof").unwrap();
            let mut client = UnixStream::connect(server.socket_path()).unwrap();
            let mut frames = encode(&Request::new("one", Method::Version)).unwrap();
            frames.extend(encode(&Request::new("two", Method::Version)).unwrap());
            if truncated_tail {
                frames.extend([0, 0, 0, 10, b'{']);
            }
            client.write_all(&frames).unwrap();
            client.shutdown(Shutdown::Write).unwrap();
            let mut count = 0;
            for _ in 0..4 {
                server.poll(Instant::now(), |r| {
                    count += 1;
                    Response::success(r.id, 0, Value::Null)
                });
            }
            assert_eq!(count, 2);
            assert_eq!(receive(&mut client).id, "one");
            assert_eq!(receive(&mut client).id, "two");
            assert_eq!(server.connection_count(), 0);
            assert_eq!(server.stats().protocol_errors, u64::from(truncated_tail));
        }
    }
    #[test]
    fn transport_errors_follow_current_sequence_across_requests_and_connections() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "sequence").unwrap();
        server.set_state_sequence(10);
        let mut first = UnixStream::connect(server.socket_path()).unwrap();
        let mut bytes = encode(&Request::new("mutate", Method::ConfigReload)).unwrap();
        let mut bad = Request::new("bad", Method::Version);
        bad.version = 2;
        bytes.extend(encode(&bad).unwrap());
        first.write_all(&bytes).unwrap();
        let mut second = UnixStream::connect(server.socket_path()).unwrap();
        bad.id = "cross-client".into();
        second.write_all(&encode(&bad).unwrap()).unwrap();
        server.poll(Instant::now(), |r| Response::success(r.id, 11, Value::Null));
        assert_eq!(receive(&mut first).state_sequence, Sequence(11));
        assert_eq!(receive(&mut first).state_sequence, Sequence(11));
        assert_eq!(receive(&mut second).state_sequence, Sequence(11));
    }
    #[test]
    fn outgoing_depth_size_and_escaped_strings_match_decoder() {
        for depth in 0..=MAX_DEPTH + 2 {
            let mut value = Value::Null;
            for _ in 0..depth {
                value = Value::Array(vec![value]);
            }
            let raw = serde_json::to_vec(&value).unwrap();
            assert_eq!(
                encode(&value).is_ok(),
                decode::<Value>(&raw).is_ok(),
                "depth {depth}"
            );
        }
        for value in [
            serde_json::json!({"[{\\\"":"{{[[:"}),
            Value::String("x".repeat(MAX_PAYLOAD - 2)),
        ] {
            let encoded = encode(&value).unwrap();
            assert_eq!(decode::<Value>(&encoded[4..]).unwrap(), value);
        }
        assert!(encode(&Value::String("x".repeat(MAX_PAYLOAD - 1))).is_err());
        let mut writer = BoundedJson {
            bytes: Vec::new(),
            containers: Vec::new(),
            in_string: false,
            escaped: false,
        };
        assert!(writer.write(&vec![b'x'; MAX_PAYLOAD + 1]).is_err());
        assert!(writer.bytes.is_empty());
    }
    #[test]
    fn oversized_handler_reply_is_typed_error_without_losing_prior_replies() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "oversize").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        client
            .write_all(&encode(&Request::new("huge", Method::Windows)).unwrap())
            .unwrap();
        server.poll(Instant::now(), |r| {
            Response::success(r.id, 1, Value::String("x".repeat(MAX_PAYLOAD)))
        });
        let response = receive(&mut client);
        assert_eq!(response.state_sequence, Sequence(1));
        assert!(matches!(
            response.outcome,
            Outcome::Failure {
                error: ErrorBody {
                    code: ErrorCode::InternalError,
                    ..
                }
            }
        ));
        assert_eq!(server.connection_count(), 1);
    }
    #[test]
    fn config_path_is_nonempty_nul_free_and_bounded() {
        for path in ["".into(), "bad\0path".into(), "x".repeat(4097)] {
            let mut r = Request::new("path", Method::ConfigCheck);
            r.params = serde_json::json!({"path":path});
            assert!(r.validate().is_err());
        }
        let mut r = Request::new("path", Method::ConfigCheck);
        assert!(r.validate().is_ok());
        r.params = serde_json::json!({"path":"relative-is-handler-policy.lua"});
        assert!(r.validate().is_ok());
    }
    #[test]
    fn bind_setup_failure_cleans_only_its_own_socket() {
        let temp = Temp::new();
        let result = Server::bind_with_setup(&temp.0, "failed", |_| {
            Err(io::Error::other("injected after bind"))
        });
        assert!(result.is_err());
        assert!(!temp.0.join("fluxwm/failed.sock").exists());
        let server = Server::bind(&temp.0, "replacement").unwrap();
        let path = server.socket_path().to_owned();
        fs::remove_file(&path).unwrap();
        let replacement = UnixListener::bind(&path).unwrap();
        drop(server);
        assert!(path.exists());
        drop(replacement);
    }
    #[test]
    fn buffered_requests_schedule_continuation_without_a_new_fd_edge() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "buffered").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        let mut bytes = Vec::new();
        for n in 0..40 {
            bytes.extend(encode(&Request::new(n.to_string(), Method::Version)).unwrap());
        }
        client.write_all(&bytes).unwrap();
        let now = Instant::now();
        let mut calls = 0;
        server.poll(now, |r| {
            calls += 1;
            Response::success(r.id, 0, Value::Null)
        });
        assert_eq!(calls, 16);
        assert!(server.readiness(now).needs_dispatch);
        while server.readiness(now).needs_dispatch {
            server.poll(now, |r| {
                calls += 1;
                Response::success(r.id, 0, Value::Null)
            });
        }
        assert_eq!(calls, 40);
        for _ in 0..40 {
            receive(&mut client);
        }
        assert!(!server.readiness(now).needs_dispatch);
    }
    #[test]
    fn accept_exhaustion_backs_off_and_rearms_with_a_deadline() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "backoff").unwrap();
        let listener_fd = server.listener.as_raw_fd();
        // A duplicated fd is not a listening socket. This deterministically injects
        // an accept error without lowering the process-wide fd limit.
        let original = unsafe { libc::dup(listener_fd) };
        assert!(original >= 0);
        let file = fs::File::open("/dev/null").unwrap();
        assert!(unsafe { libc::dup2(file.as_raw_fd(), listener_fd) } >= 0);
        let now = Instant::now();
        server.poll(now, |_| unreachable!());
        let ready = server.readiness(now);
        assert_eq!(server.stats().accept_errors, 1);
        assert!(ready.interests.is_empty());
        assert!(ready.next_deadline.is_some_and(|t| t > now));
        server.poll(now, |_| unreachable!());
        assert_eq!(server.stats().accept_errors, 1);
        assert!(unsafe { libc::dup2(original, listener_fd) } >= 0);
        unsafe { libc::close(original) };
        server.poll(now + Duration::from_millis(100), |_| unreachable!());
        assert!(
            server
                .readiness(now + Duration::from_millis(100))
                .interests
                .iter()
                .any(|i| i.endpoint == Endpoint::Listener)
        );
    }
    #[test]
    fn client_total_deadline_bounds_trickle_prefix_and_full_connect_backlog() {
        let temp = Temp::new();
        let path = temp.0.join("trickle.sock");
        let listener = UnixListener::bind(&path).unwrap();
        let worker = std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            for byte in [0, 0, 0, 20] {
                if client.write_all(&[byte]).is_err() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(30));
            }
        });
        let started = Instant::now();
        let error = call_with_timeout(
            &path,
            &Request::new("trickle", Method::Version),
            Duration::from_millis(55),
        )
        .unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::TimedOut);
        assert!(started.elapsed() < Duration::from_secs(1));
        worker.join().unwrap();
        let path = temp.0.join("full.sock");
        let listener = UnixListener::bind(&path).unwrap();
        assert_eq!(unsafe { libc::listen(listener.as_raw_fd(), 0) }, 0);
        let _filled = UnixStream::connect(&path).unwrap();
        let started = Instant::now();
        assert_eq!(
            call_with_timeout(
                &path,
                &Request::new("full", Method::Version),
                Duration::from_millis(30)
            )
            .unwrap_err()
            .kind(),
            io::ErrorKind::TimedOut
        );
        assert!(started.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn recursive_serializer_stops_before_unbounded_recursion() {
        struct Recursive;
        impl Serialize for Recursive {
            fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeSeq;
                let mut seq = s.serialize_seq(Some(1))?;
                seq.serialize_element(self)?;
                seq.end()
            }
        }
        assert!(encode(&Recursive).is_err());
    }
    #[test]
    fn half_close_preserves_large_pending_response_until_fully_flushed() {
        use std::net::Shutdown;
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "large-eof").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        client
            .write_all(&encode(&Request::new("large", Method::Windows)).unwrap())
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        client.set_nonblocking(true).unwrap();
        let mut captured = Vec::new();
        let mut count = 0;
        let now = Instant::now();
        for _ in 0..100 {
            server.poll(now, |r| {
                count += 1;
                Response::success(r.id, 1, Value::String("x".repeat(IO_BUDGET * 3)))
            });
            let mut bytes = [0; 8192];
            loop {
                match client.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(n) => captured.extend_from_slice(&bytes[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => panic!("{e}"),
                }
            }
            if server.connection_count() == 0 {
                break;
            }
        }
        assert_eq!(count, 1);
        assert_eq!(server.connection_count(), 0);
        let mut decoder = Decoder::default();
        decoder.push(&captured).unwrap();
        let response: Response = decode(&decoder.next_frame().unwrap().unwrap()).unwrap();
        assert_eq!(response.id, "large");
        assert!(decoder.is_empty());
    }
    #[test]
    fn socket_action_admission_uses_shared_strict_fields_and_domain_policy() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "actions").unwrap();
        server.set_state_sequence(8);
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        let mut actions = vec![
            serde_json::json!({"name":"pan","dx":1e100,"dy":0}),
            serde_json::json!({"name":"spawn","argv":[]}),
        ];
        for name in ["quit", "lock", "reload-config", "close-window"] {
            actions.push(serde_json::json!({"name":name,"unexpected":true}));
        }
        for (index, params) in actions.iter().enumerate() {
            let mut request = Request::new(index.to_string(), Method::Action);
            request.params = params.clone();
            client.write_all(&encode(&request).unwrap()).unwrap();
        }
        server.poll(Instant::now(), |_| panic!("invalid action reached handler"));
        for _ in actions {
            let response = receive(&mut client);
            assert_eq!(response.state_sequence, Sequence(8));
            assert!(matches!(
                response.outcome,
                Outcome::Failure {
                    error: ErrorBody {
                        code: ErrorCode::InvalidParams,
                        ..
                    }
                }
            ));
        }
    }
}
