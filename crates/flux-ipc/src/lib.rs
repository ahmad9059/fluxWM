//! Bounded local IPC. Call `Server::poll` from the compositor event loop.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{HashSet, VecDeque},
    fs,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::{
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
                || obj
                    .get("path")
                    .is_some_and(|v| !v.is_string() || v.as_str().unwrap().len() > 4096)
            {
                return Err(ErrorBody::new(
                    ErrorCode::InvalidParams,
                    "config.check accepts only optional string path",
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
pub fn encode(value: &impl Serialize) -> io::Result<Vec<u8>> {
    let payload = serde_json::to_vec(value).map_err(|e| invalid(e.to_string()))?;
    if payload.is_empty() || payload.len() > MAX_PAYLOAD {
        return Err(invalid("invalid frame length"));
    }
    let mut b = Vec::with_capacity(payload.len() + 4);
    b.extend_from_slice(&(payload.len() as u32).to_be_bytes());
    b.extend(payload);
    Ok(b)
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
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

struct Pending {
    bytes: Vec<u8>,
    offset: usize,
    id: String,
}
struct Connection {
    stream: UnixStream,
    decoder: Decoder,
    out: VecDeque<Pending>,
    ids: HashSet<String>,
    queued: usize,
    partial_since: Option<Instant>,
    write_since: Option<Instant>,
    last_activity: Instant,
}
pub struct Server {
    listener: UnixListener,
    path: PathBuf,
    identity: (u64, u64),
    connections: Vec<Connection>,
    state_sequence: u64,
}
impl Server {
    /// The runtime directory and its fluxwm child must be owned by the effective UID,
    /// without group/other access. Existing socket paths are never replaced.
    pub fn bind(runtime_dir: &Path, instance: &str) -> io::Result<Self> {
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
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600))?;
        listener.set_nonblocking(true)?;
        let meta = fs::symlink_metadata(&path)?;
        Ok(Self {
            listener,
            path,
            identity: (meta.dev(), meta.ino()),
            connections: Vec::new(),
            state_sequence: 0,
        })
    }
    pub fn socket_path(&self) -> &Path {
        &self.path
    }
    /// Snapshot sequence used in transport-generated validation errors.
    pub fn set_state_sequence(&mut self, sequence: u64) {
        self.state_sequence = sequence;
    }
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
    /// Work is bounded per call (32 accepts, 64 KiB IO and 16 requests per client).
    /// Handler must only snapshot state or enqueue commands; never perform blocking IO.
    pub fn poll(&mut self, now: Instant, mut handler: impl FnMut(Request) -> Response) {
        for _ in 0..MAX_CONNECTIONS {
            match self.listener.accept() {
                Ok((stream, _)) => {
                    if self.connections.len() >= MAX_CONNECTIONS || !same_uid(&stream) {
                        continue;
                    }
                    if stream.set_nonblocking(true).is_err() {
                        continue;
                    }
                    self.connections.push(Connection {
                        stream,
                        decoder: Decoder::default(),
                        out: VecDeque::new(),
                        ids: HashSet::new(),
                        queued: 0,
                        partial_since: None,
                        write_since: None,
                        last_activity: now,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        self.connections
            .retain_mut(|c| process(c, now, self.state_sequence, &mut handler).is_ok());
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if let Ok(m) = fs::symlink_metadata(&self.path)
            && (m.dev(), m.ino()) == self.identity
        {
            let _ = fs::remove_file(&self.path);
        }
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
fn process(
    c: &mut Connection,
    now: Instant,
    state_sequence: u64,
    handler: &mut impl FnMut(Request) -> Response,
) -> io::Result<()> {
    if now.saturating_duration_since(c.last_activity) > IDLE_DEADLINE
        || c.partial_since
            .is_some_and(|t| now.saturating_duration_since(t) > DEADLINE)
        || c.write_since
            .is_some_and(|t| now.saturating_duration_since(t) > DEADLINE)
    {
        return Err(invalid("IPC deadline exceeded"));
    }
    let mut buffer = [0u8; 8192];
    let mut read = 0;
    while read < IO_BUDGET {
        match c.stream.read(&mut buffer) {
            Ok(0) => return Err(io::ErrorKind::UnexpectedEof.into()),
            Ok(n) => {
                read += n;
                c.last_activity = now;
                if c.decoder.is_empty() {
                    c.partial_since = Some(now);
                }
                c.decoder.push(&buffer[..n])?;
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    for _ in 0..16 {
        let Some(frame) = c.decoder.next_frame()? else {
            break;
        };
        let request: Request = decode(&frame)?;
        if request.id.is_empty() || request.id.len() > 128 || !c.ids.insert(request.id.clone()) {
            return Err(invalid("invalid or duplicate outstanding request ID"));
        }
        let id = request.id.clone();
        let response = match request.validate() {
            Ok(()) => handler(request),
            Err(error) => Response::error(&id, state_sequence, error),
        };
        if response.id != id || response.version != VERSION {
            return Err(invalid("invalid handler response"));
        }
        let bytes = encode(&response)?;
        if c.out.len() >= 256 || c.queued + bytes.len() > MAX_QUEUE {
            return Err(invalid("slow consumer queue exceeded"));
        }
        c.queued += bytes.len();
        c.out.push_back(Pending {
            bytes,
            offset: 0,
            id,
        });
        if c.write_since.is_none() {
            c.write_since = Some(now);
        }
    }
    if c.decoder.is_empty() {
        c.partial_since = None;
    }
    let mut written = 0;
    while written < IO_BUDGET {
        let Some(front) = c.out.front_mut() else {
            break;
        };
        let end = (front.offset + IO_BUDGET - written).min(front.bytes.len());
        match c.stream.write(&front.bytes[front.offset..end]) {
            Ok(0) => return Err(io::ErrorKind::WriteZero.into()),
            Ok(n) => {
                front.offset += n;
                written += n;
                c.queued -= n;
                if front.offset == front.bytes.len() {
                    let p = c.out.pop_front().unwrap();
                    c.ids.remove(&p.id);
                }
            }
            Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    if c.out.is_empty() {
        c.write_since = None;
    }
    Ok(())
}
/// Blocking client helper with bounded message size and five second IO deadlines.
/// Intended for fluxctl, never the compositor thread.
pub fn call(path: &Path, request: &Request) -> io::Result<Response> {
    request.validate().map_err(|e| invalid(e.message))?;
    let mut stream = UnixStream::connect(path)?;
    stream.set_read_timeout(Some(DEADLINE))?;
    stream.set_write_timeout(Some(DEADLINE))?;
    stream.write_all(&encode(request)?)?;
    let mut prefix = [0; 4];
    stream.read_exact(&mut prefix)?;
    let size = u32::from_be_bytes(prefix) as usize;
    if size == 0 || size > MAX_PAYLOAD {
        return Err(invalid("invalid response length"));
    }
    let mut b = vec![0; size];
    stream.read_exact(&mut b)?;
    let response: Response = decode(&b)?;
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
        server.poll(now, |r| Response::success(r.id, 0, Value::Null));
        assert_eq!(server.connection_count(), 0);
    }
    #[test]
    fn slow_reader_queue_disconnect() {
        let temp = Temp::new();
        let mut server = Server::bind(&temp.0, "slow").unwrap();
        let mut client = UnixStream::connect(server.socket_path()).unwrap();
        for id in ["1", "2", "3"] {
            client
                .write_all(&encode(&Request::new(id, Method::Version)).unwrap())
                .unwrap();
        }
        server.poll(Instant::now(), |r| {
            Response::success(r.id, 0, Value::String("x".repeat(MAX_PAYLOAD / 2)))
        });
        assert_eq!(server.connection_count(), 0);
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
}
