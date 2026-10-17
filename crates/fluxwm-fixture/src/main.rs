//! Original synthetic protocol client for FluxWM's nested developer smoke tests.
use serde_json::{Value as Json, json};
use std::{
    fs::File,
    io::{Read, Write},
    os::{
        fd::{AsFd, AsRawFd, FromRawFd, OwnedFd},
        unix::{ffi::OsStrExt, net::UnixStream},
    },
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use wayland_client::{
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{xdg_surface, xdg_toplevel, xdg_wm_base};

type Result<T> = std::result::Result<T, String>;
const WIDTH: i32 = 320;
const HEIGHT: i32 = 240;
#[derive(Clone, Copy, Debug)]
enum Callback {
    Sync(u32),
    Frame(usize),
}
struct Window {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    buffer: Option<wl_buffer::WlBuffer>,
    _storage: Option<File>,
    configures: u32,
    frames: u32,
    capabilities: u32,
    destroyed: bool,
}
struct State {
    start: Instant,
    globals: Vec<Json>,
    events: Vec<Json>,
    compositor: Option<wl_compositor::WlCompositor>,
    shm: Option<wl_shm::WlShm>,
    wm: Option<xdg_wm_base::XdgWmBase>,
    seat: Option<wl_seat::WlSeat>,
    formats: Vec<u32>,
    windows: Vec<Window>,
    last_sync: u32,
    sync_done_count: u32,
    next_sync: u32,
    error: Option<String>,
    invalid_move_attempted: bool,
    pointer_feedback_enabled: bool,
    pointer: Option<wl_pointer::WlPointer>,
    pointer_on_primary: bool,
    pointer_xy: Option<(f64, f64)>,
    pointer_enters: u32,
    pointer_leaves: u32,
    stationary_motions: u32,
    changing_motions: u32,
    commit_on_motion: bool,
    feedback_commits: u32,
}
impl State {
    fn new(start: Instant) -> Self {
        Self {
            start,
            globals: vec![],
            events: vec![],
            compositor: None,
            shm: None,
            wm: None,
            seat: None,
            formats: vec![],
            windows: vec![],
            last_sync: 0,
            sync_done_count: 0,
            next_sync: 1,
            error: None,
            invalid_move_attempted: false,
            pointer_feedback_enabled: false,
            pointer: None,
            pointer_on_primary: false,
            pointer_xy: None,
            pointer_enters: 0,
            pointer_leaves: 0,
            stationary_motions: 0,
            changing_motions: 0,
            commit_on_motion: false,
            feedback_commits: 0,
        }
    }
    fn record(&mut self, event: Json) {
        if self.events.len() < 1024 {
            self.events
                .push(json!({"at_ms":self.start.elapsed().as_millis(),"event":event}));
        } else {
            self.error = Some("event evidence limit exceeded".into());
        }
    }
    fn sync(&mut self, connection: &Connection, qh: &QueueHandle<Self>) -> u32 {
        let n = self.next_sync;
        self.next_sync += 1;
        connection.display().sync(qh, Callback::Sync(n));
        n
    }
    fn create(&mut self, qh: &QueueHandle<Self>, name: &str) -> usize {
        let n = self.windows.len();
        let surface = self.compositor.as_ref().unwrap().create_surface(qh, ());
        let xdg = self.wm.as_ref().unwrap().get_xdg_surface(&surface, qh, n);
        let top = xdg.get_toplevel(qh, n);
        top.set_title(format!("FluxWM fixture {name}"));
        top.set_app_id("org.fluxwm.Fixture".into());
        self.windows.push(Window {
            surface,
            xdg,
            toplevel: top,
            buffer: None,
            _storage: None,
            configures: 0,
            frames: 0,
            capabilities: 0,
            destroyed: false,
        });
        self.record(json!({"create":n,"name":name}));
        n
    }
    fn map(&mut self, index: usize, qh: &QueueHandle<Self>) -> Result<()> {
        if self.windows[index].configures == 0 {
            return Err("attempted map before configure acknowledgement".into());
        }
        if self.windows[index].buffer.is_none() {
            let bytes = pixels(index);
            let size = i32::try_from(bytes.len()).map_err(|e| e.to_string())?;
            let mut file = memory_file()?;
            file.set_len(size as u64).map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            let pool = self
                .shm
                .as_ref()
                .unwrap()
                .create_pool(file.as_fd(), size, qh, ());
            let buffer = pool.create_buffer(
                0,
                WIDTH,
                HEIGHT,
                WIDTH * 4,
                wl_shm::Format::Xrgb8888,
                qh,
                (),
            );
            pool.destroy();
            self.windows[index].buffer = Some(buffer);
            self.windows[index]._storage = Some(file);
        }
        let w = &self.windows[index];
        w.xdg.set_window_geometry(0, 0, WIDTH, HEIGHT);
        w.surface.attach(w.buffer.as_ref(), 0, 0);
        w.surface.damage(0, 0, WIDTH, HEIGHT);
        w.surface.frame(qh, Callback::Frame(index));
        w.surface.commit();
        self.record(json!({"map_commit":index}));
        Ok(())
    }
}
fn pixels(index: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity((WIDTH * HEIGHT * 4) as usize);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let rgb = if index == 0 {
                match (x < WIDTH / 2, y < HEIGHT / 2) {
                    (true, true) => [255, 0, 0],
                    (false, true) => [0, 255, 0],
                    (true, false) => [0, 0, 255],
                    (false, false) => [255, 255, 0],
                }
            } else {
                [255, 0, 255]
            };
            let pixel =
                0xff00_0000u32 | ((rgb[0] as u32) << 16) | ((rgb[1] as u32) << 8) | rgb[2] as u32;
            out.extend_from_slice(&pixel.to_ne_bytes());
        }
    }
    out
}
fn memory_file() -> Result<File> {
    // SAFETY: valid static C string and flags; ownership is established only on success.
    let fd = unsafe { libc::memfd_create(c"fluxwm-fixture-shm".as_ptr(), libc::MFD_CLOEXEC) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    Ok(unsafe { File::from_raw_fd(fd) })
}
fn poll_fd(fd: i32, events: i16, deadline: Instant) -> Result<i16> {
    loop {
        let Some(left) = deadline.checked_duration_since(Instant::now()) else {
            return Err("fixture deadline expired".into());
        };
        let timeout = left.as_millis().clamp(1, i32::MAX as u128) as i32;
        let mut descriptor = libc::pollfd {
            fd,
            events,
            revents: 0,
        };
        // SAFETY: one initialized pollfd with a valid live owned fd; bounded timeout.
        let result = unsafe { libc::poll(&mut descriptor, 1, timeout) };
        if result > 0 {
            return Ok(descriptor.revents);
        }
        if result == 0 {
            // poll's millisecond timeout can round down; recheck the absolute
            // deadline rather than reporting an early timeout.
            continue;
        }
        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(error.to_string());
        }
    }
}
fn display_path() -> Result<PathBuf> {
    let display = std::env::var_os("WAYLAND_DISPLAY")
        .ok_or("WAYLAND_DISPLAY must explicitly name the nested test socket")?;
    if display.is_empty() {
        return Err("WAYLAND_DISPLAY must not be empty".into());
    }
    let display = PathBuf::from(display);
    if display.is_absolute() {
        Ok(display)
    } else {
        let runtime = std::env::var_os("XDG_RUNTIME_DIR")
            .ok_or("relative WAYLAND_DISPLAY requires XDG_RUNTIME_DIR")?;
        Ok(PathBuf::from(runtime).join(display))
    }
}
fn connect_stream(path: &Path, deadline: Instant) -> Result<UnixStream> {
    let bytes = path.as_os_str().as_bytes();
    // SAFETY: sockaddr_un is an ordinary zero-initializable C record.
    let mut address: libc::sockaddr_un = unsafe { std::mem::zeroed() };
    if bytes.len() >= address.sun_path.len() || bytes.contains(&0) {
        return Err("Wayland socket path is too long or contains NUL".into());
    }
    address.sun_family = libc::AF_UNIX as _;
    for (destination, byte) in address.sun_path.iter_mut().zip(bytes) {
        *destination = *byte as _;
    }
    // SAFETY: socket arguments are valid constants. The returned fd is owned once.
    let raw = unsafe {
        libc::socket(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
        )
    };
    if raw < 0 {
        return Err(std::io::Error::last_os_error().to_string());
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw) };
    let len =
        (std::mem::offset_of!(libc::sockaddr_un, sun_path) + bytes.len() + 1) as libc::socklen_t;
    // SAFETY: address and its initialized pathname span the declared sockaddr length.
    let status = unsafe {
        libc::connect(
            fd.as_raw_fd(),
            (&address as *const libc::sockaddr_un).cast(),
            len,
        )
    };
    if status < 0 {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() != Some(libc::EINPROGRESS) {
            return Err(format!("connect {}: {error}", path.display()));
        }
        poll_fd(fd.as_raw_fd(), libc::POLLOUT, deadline)?;
        let mut error = 0i32;
        let mut size = std::mem::size_of_val(&error) as libc::socklen_t;
        // SAFETY: getsockopt writes to correctly sized initialized scalar storage.
        if unsafe {
            libc::getsockopt(
                fd.as_raw_fd(),
                libc::SOL_SOCKET,
                libc::SO_ERROR,
                (&mut error as *mut i32).cast(),
                &mut size,
            )
        } < 0
        {
            return Err(std::io::Error::last_os_error().to_string());
        }
        if error != 0 {
            return Err(std::io::Error::from_raw_os_error(error).to_string());
        }
    }
    Ok(UnixStream::from(fd))
}
fn connect(path: &Path, deadline: Instant) -> Result<Connection> {
    Connection::from_socket(connect_stream(path, deadline)?).map_err(|e| e.to_string())
}
fn until(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
    label: &str,
    done: impl Fn(&State) -> bool,
) -> Result<()> {
    loop {
        if Instant::now() >= deadline {
            return Err(format!("{label}: fixture deadline expired"));
        }
        queue
            .dispatch_pending(state)
            .map_err(|e| format!("{label}: {e}"))?;
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        if done(state) {
            return Ok(());
        }
        let output_pending = match queue.flush() {
            Ok(()) => false,
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                true
            }
            Err(e) => return Err(format!("{label}: {e}")),
        };
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let events = poll_fd(
            connection.as_fd().as_raw_fd(),
            libc::POLLIN | if output_pending { libc::POLLOUT } else { 0 },
            deadline,
        )
        .map_err(|e| format!("{label}: {e}"))?;
        if events & libc::POLLIN != 0 {
            match guard.read() {
                Ok(_) => (),
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(format!("{label}: {e}")),
            }
        } else {
            drop(guard);
            if events & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return Err(format!("{label}: socket closed/error {events}"));
            }
        }
    }
}
impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        {
            if state.globals.len() >= 256 {
                state.error = Some("registry evidence limit exceeded".into());
                return;
            }
            state
                .globals
                .push(json!({"name":name,"interface":interface,"version":version}));
            match interface.as_str() {
                "wl_compositor" if state.compositor.is_none() => {
                    state.compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "wl_shm" if state.shm.is_none() => state.shm = Some(registry.bind(name, 1, qh, ())),
                "xdg_wm_base" if state.wm.is_none() => {
                    state.wm = Some(registry.bind(name, version.min(7), qh, ()))
                }
                "wl_seat" if state.seat.is_none() => {
                    state.seat = Some(registry.bind(name, version.min(7), qh, ()))
                }
                _ => (),
            }
        }
    }
}
impl Dispatch<xdg_wm_base::XdgWmBase, ()> for State {
    fn event(
        _: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            wm.pong(serial);
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, usize> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.windows[*index].configures += 1;
            state.record(json!({"configure_ack":index,"serial":serial}));
        }
    }
}
impl Dispatch<xdg_toplevel::XdgToplevel, usize> for State {
    fn event(
        state: &mut Self,
        _: &xdg_toplevel::XdgToplevel,
        event: xdg_toplevel::Event,
        index: &usize,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_toplevel::Event::WmCapabilities { capabilities } => {
                state.windows[*index].capabilities += 1;
                state.record(json!({"wm_capabilities":index,"bytes":capabilities}));
                if !capabilities.is_empty() {
                    state.error = Some(format!(
                        "window {index}: expected no optional toplevel capabilities in this nested baseline"
                    ));
                }
            }
            xdg_toplevel::Event::Close => {
                state.error = Some(format!("window {index}: unexpected close"))
            }
            _ => (),
        }
    }
}
impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_shm::WlShm,
        event: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_shm::Event::Format {
            format: WEnum::Value(format),
        } = event
        {
            state.formats.push(format as u32);
        }
    }
}
impl Dispatch<wl_callback::WlCallback, Callback> for State {
    fn event(
        state: &mut Self,
        _: &wl_callback::WlCallback,
        event: wl_callback::Event,
        kind: &Callback,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { callback_data } = event {
            match kind {
                Callback::Sync(n) => {
                    state.last_sync = *n;
                    state.sync_done_count += 1;
                }
                Callback::Frame(n) => {
                    state.windows[*n].frames += 1;
                    state.record(json!({"frame_done":n,"timestamp":callback_data}));
                }
            }
        }
    }
}
delegate_noop!(State:ignore wl_compositor::WlCompositor);
delegate_noop!(State:ignore wl_surface::WlSurface);
delegate_noop!(State:ignore wl_shm_pool::WlShmPool);
delegate_noop!(State:ignore wl_buffer::WlBuffer);
impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
            && state.pointer_feedback_enabled
            && state.pointer.is_none()
            && capabilities.contains(wl_seat::Capability::Pointer)
        {
            state.pointer = Some(seat.get_pointer(qh, ()));
        }
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_on_primary =
                    state.windows.first().is_some_and(|w| w.surface == surface);
                if state.pointer_on_primary {
                    state.pointer_enters += 1;
                    state.pointer_xy = Some((surface_x, surface_y));
                    state.record(json!({"pointer_enter_primary":[surface_x,surface_y]}));
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if state.pointer_on_primary {
                    state.pointer_leaves += 1;
                }
                state.pointer_on_primary = false;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } if state.pointer_on_primary => {
                let xy = (surface_x, surface_y);
                if state.pointer_xy == Some(xy) {
                    state.stationary_motions += 1;
                } else {
                    state.changing_motions += 1;
                }
                state.pointer_xy = Some(xy);
                // Reproduce cursor-updating clients while bounding this fixture's
                // feedback-generated requests even against a broken compositor.
                if state.commit_on_motion && state.feedback_commits < 32 {
                    state.windows[0].surface.damage(0, 0, 1, 1);
                    state.windows[0].surface.commit();
                    state.feedback_commits += 1;
                }
            }
            _ => {}
        }
    }
}
fn lifecycle(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
) -> Result<()> {
    let qh = queue.handle();
    connection.display().get_registry(&qh, ());
    let sync = state.sync(connection, &qh);
    until(connection, queue, state, deadline, "registry", |s| {
        s.last_sync >= sync
    })?;
    if state.compositor.is_none() || state.wm.is_none() || state.shm.is_none() {
        return Err("required wl_compositor/xdg_wm_base/wl_shm global missing".into());
    }
    let sync = state.sync(connection, &qh);
    until(connection, queue, state, deadline, "bound globals", |s| {
        s.last_sync >= sync
    })?;
    if !state.formats.contains(&(wl_shm::Format::Xrgb8888 as u32)) {
        return Err("XRGB8888 SHM format not advertised".into());
    }
    let first = state.create(&qh, "quadrants");
    let second = state.create(&qh, "disconnect-survivor");
    if let Some(seat) = state.seat.as_ref() {
        state.windows[second].toplevel._move(seat, 0);
        state.invalid_move_attempted = true;
    }
    for w in &state.windows {
        w.surface.commit();
    }
    until(
        connection,
        queue,
        state,
        deadline,
        "initial configure",
        |s| s.windows.iter().all(|w| w.configures >= 1),
    )?;
    for (i, w) in state.windows.iter().enumerate() {
        if w.toplevel.version() >= 5 && w.capabilities == 0 {
            return Err(format!("window {i}: missing initial wm_capabilities event"));
        }
    }
    state.map(first, &qh)?;
    until(
        connection,
        queue,
        state,
        deadline,
        "primary initial frame callback",
        |s| s.windows[first].frames >= 1,
    )?;
    // Demand a second callback to prove progress beyond the first mapped frame.
    state.windows[first]
        .surface
        .frame(&qh, Callback::Frame(first));
    state.windows[first].surface.commit();
    until(
        connection,
        queue,
        state,
        deadline,
        "second frame callback",
        |s| s.windows[first].frames >= 2,
    )?;
    state.map(second, &qh)?;
    until(
        connection,
        queue,
        state,
        deadline,
        "sibling initial frame callback",
        |s| s.windows[second].frames >= 1,
    )?;
    state.windows[first].surface.attach(None, 0, 0);
    state.windows[first].surface.commit();
    state.record(json!({"null_buffer_unmap":first}));
    let sync = state.sync(connection, &qh);
    until(connection, queue, state, deadline, "unmap processed", |s| {
        s.last_sync >= sync
    })?;
    let prior = state.windows[first].configures;
    state.windows[first]
        .toplevel
        .set_title("FluxWM fixture remapped quadrants".into());
    state.windows[first]
        .toplevel
        .set_app_id("org.fluxwm.Fixture".into());
    state.windows[first].surface.commit();
    state.record(json!({"remap_initial_bufferless_commit":first}));
    until(
        connection,
        queue,
        state,
        deadline,
        "remap fresh configure",
        |s| s.windows[first].configures > prior,
    )?;
    state.map(first, &qh)?;
    until(
        connection,
        queue,
        state,
        deadline,
        "remap frame callback",
        |s| s.windows[first].frames >= 3,
    )?;
    let w = &mut state.windows[first];
    w.toplevel.destroy();
    w.xdg.destroy();
    w.surface.destroy();
    if let Some(b) = w.buffer.take() {
        b.destroy();
    }
    w.destroyed = true;
    state.record(json!({"explicit_destroy":first}));
    let sync = state.sync(connection, &qh);
    until(
        connection,
        queue,
        state,
        deadline,
        "destroy processed",
        |s| s.last_sync >= sync,
    )?;
    state.record(json!({"disconnect_with_mapped_siblings":1}));
    Ok(())
}
fn run(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let path = match display_path() {
        Ok(path) => path,
        Err(error) => return json!({"ok":false,"error":error}),
    };
    let connection = match connect(&path, deadline) {
        Ok(connection) => connection,
        Err(error) => return json!({"ok":false,"error":error}),
    };
    let mut queue = connection.new_event_queue::<State>();
    let mut state = State::new(start);
    let result = lifecycle(&connection, &mut queue, &mut state, deadline);
    // Dropping the queue/connection after this evidence intentionally disconnects
    // with the second window still mapped; server stats must prove cleanup.
    json!({"ok":result.is_ok(),"error":result.err(),"mode":"lifecycle","socket":path,"elapsed_ms":start.elapsed().as_millis(),
        "wm_base_bound_version":state.wm.as_ref().map(Proxy::version),"invalid_unmapped_move_attempted":state.invalid_move_attempted,
        "globals":state.globals,"events":state.events,"windows":state.windows.iter().enumerate().map(|(i,w)|json!({"index":i,"configure_acks":w.configures,"frame_callbacks":w.frames,"wm_capability_events":w.capabilities,"explicitly_destroyed":w.destroyed})).collect::<Vec<_>>(),
        "expected_primary_quadrants_rgb":[[255,0,0],[0,255,0],[0,0,255],[255,255,0]],"expected_sibling_rgb":[255,0,255]})
}

/// A persistent out-of-band channel. No connection close/request is generated
/// while the slow Wayland connection is draining its final marker.
struct Diagnostics {
    socket: UnixStream,
    next_id: u32,
}
impl Diagnostics {
    fn new(path: &Path, deadline: Instant) -> Result<Self> {
        Ok(Self {
            socket: connect_stream(path, deadline)?,
            next_id: 0,
        })
    }
    fn snapshot(&mut self, deadline: Instant) -> Result<Json> {
        let id = format!("fixture-{}", self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("diagnostic request identity capacity exhausted")?;
        let payload =
            serde_json::to_vec(&json!({"version":1,"id":id,"method":"diagnostics","params":{}}))
                .map_err(|e| e.to_string())?;
        let mut frame = (payload.len() as u32).to_be_bytes().to_vec();
        frame.extend(payload);
        transfer(&mut self.socket, &mut frame, true, deadline)?;
        let mut header = [0; 4];
        transfer(&mut self.socket, &mut header, false, deadline)?;
        let length = u32::from_be_bytes(header) as usize;
        if length == 0 || length > 65536 {
            return Err("diagnostic response outside 1..65536 bytes".into());
        }
        let mut payload = vec![0; length];
        transfer(&mut self.socket, &mut payload, false, deadline)?;
        let response: Json = serde_json::from_slice(&payload).map_err(|e| e.to_string())?;
        if response.get("id") != Some(&json!(id)) {
            return Err("diagnostics response identity mismatch".into());
        }
        let stats = response
            .pointer("/result/runtime")
            .cloned()
            .ok_or_else(|| format!("missing diagnostic runtime counters: {response}"))?;
        for name in [
            "wayland_requests",
            "flush_cycles",
            "flush_would_block",
            "flush_retry_timer_fired",
        ] {
            if stats.get(name).and_then(Json::as_u64).is_none() {
                return Err(format!("missing numeric diagnostic {name}"));
            }
        }
        Ok(stats)
    }
    fn barrier(&mut self, target: u64, deadline: Instant) -> Result<Json> {
        let observed = loop {
            let stats = self.snapshot(deadline)?;
            if counter(&stats, "wayland_requests") >= target {
                break stats;
            }
            std::thread::sleep(Duration::from_millis(1));
        };
        loop {
            let stats = self.snapshot(deadline)?;
            if counter(&stats, "wayland_requests") >= target
                && counter(&stats, "flush_cycles") > counter(&observed, "flush_cycles")
            {
                return Ok(stats);
            }
            std::thread::sleep(Duration::from_millis(1));
        }
    }
}
fn counter(value: &Json, name: &str) -> u64 {
    value[name].as_u64().unwrap_or(0)
}
fn transfer(
    socket: &mut UnixStream,
    bytes: &mut [u8],
    write: bool,
    deadline: Instant,
) -> Result<()> {
    let mut offset = 0;
    while offset < bytes.len() {
        if Instant::now() >= deadline {
            return Err("diagnostics transfer deadline expired".into());
        }
        let result = if write {
            socket.write(&bytes[offset..])
        } else {
            socket.read(&mut bytes[offset..])
        };
        match result {
            Ok(0) => return Err("diagnostics socket closed during frame".into()),
            Ok(n) => offset += n,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                poll_fd(
                    socket.as_raw_fd(),
                    if write { libc::POLLOUT } else { libc::POLLIN },
                    deadline,
                )?;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => return Err(e.to_string()),
        }
    }
    Ok(())
}
fn flush_without_reading(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    deadline: Instant,
) -> Result<()> {
    loop {
        if Instant::now() >= deadline {
            return Err("request flush deadline expired".into());
        }
        match queue.flush() {
            Ok(()) => return Ok(()),
            Err(wayland_client::backend::WaylandError::Io(e))
                if e.kind() == std::io::ErrorKind::WouldBlock =>
            {
                poll_fd(connection.as_fd().as_raw_fd(), libc::POLLOUT, deadline)?;
            }
            Err(e) => return Err(format!("request flush: {e}")),
        }
    }
}
fn slow_reader(timeout: Duration) -> Json {
    const BATCH: u32 = 32;
    const MAX_BATCHES: u32 = 512;
    let start = Instant::now();
    let deadline = start + timeout;
    let mut evidence = json!({"mode":"slow-reader","ok":false,"batch_callbacks":BATCH,"max_batches":MAX_BATCHES,"max_callbacks":BATCH*MAX_BATCHES+1,"read_delay_ms":200,"server_backpressure_proven":false});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        evidence["socket"] = json!(path);
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let mut state = State::new(start);
        let ipc = std::env::var_os("FLUXWM_SOCKET")
            .ok_or("slow-reader requires FLUXWM_SOCKET diagnostics from the nested compositor")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let baseline = diagnostics.snapshot(deadline)?;
        let base_requests = counter(&baseline, "wayland_requests");
        evidence["baseline"] = baseline.clone();
        let mut requests = 0;
        let mut batches = 0;
        let blocked = loop {
            if batches >= MAX_BATCHES {
                return Err("bounded request ceiling reached without proven server EAGAIN; not a passing backpressure test".into());
            }
            for _ in 0..BATCH {
                state.sync(&connection, &qh);
            }
            requests += BATCH;
            batches += 1;
            flush_without_reading(&connection, &mut queue, deadline)?;
            let stats = diagnostics.barrier(base_requests + u64::from(requests), deadline)?;
            if counter(&stats, "flush_would_block") > counter(&baseline, "flush_would_block") {
                break stats;
            }
        };
        evidence["batches_to_backpressure"] = json!(batches);
        evidence["callback_requests_before_marker"] = json!(requests);
        evidence["blocked_snapshot"] = blocked;
        evidence["server_backpressure_proven"] = json!(true);
        let marker = state.sync(&connection, &qh);
        requests += 1;
        flush_without_reading(&connection, &mut queue, deadline)?;
        let marker_processed =
            diagnostics.barrier(base_requests + u64::from(requests), deadline)?;
        evidence["marker_processed_snapshot"] = marker_processed;
        // All request generation stops here. The following delay gives the server
        // time to retry against the full socket; no Wayland reads occur until after it.
        if deadline.saturating_duration_since(Instant::now()) < Duration::from_millis(300) {
            return Err("insufficient remaining deadline for delayed drain".into());
        }
        std::thread::sleep(Duration::from_millis(200));
        let drain_start = Instant::now();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "delayed final marker",
            |s| s.last_sync >= marker,
        )?;
        evidence["drain_ms"] = json!(drain_start.elapsed().as_millis());
        evidence["marker_received"] = json!(marker);
        evidence["callback_events_received"] = json!(state.sync_done_count);
        evidence["no_requests_during_marker_drain"] = json!(true);
        if state.sync_done_count != requests {
            return Err(format!(
                "expected {requests} callback events, received {}",
                state.sync_done_count
            ));
        }
        // This IPC request is after marker receipt; it cannot cause its delivery.
        let final_stats = diagnostics.snapshot(deadline)?;
        if counter(&final_stats, "wayland_requests") != base_requests + u64::from(requests) {
            return Err("unexpected concurrent Wayland requests; run this fixture in an isolated nested instance".into());
        }
        if counter(&final_stats, "flush_retry_timer_fired")
            <= counter(&baseline, "flush_retry_timer_fired")
        {
            return Err("no retry timer execution was observed".into());
        }
        evidence["final_snapshot"] = final_stats;
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}
fn observe_for(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    duration: Duration,
    deadline: Instant,
) -> Result<()> {
    let stop = Instant::now()
        .checked_add(duration)
        .ok_or("observation deadline overflow")?;
    if stop > deadline {
        return Err("insufficient fixture deadline for observation interval".into());
    }
    loop {
        queue.dispatch_pending(state).map_err(|e| e.to_string())?;
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        flush_without_reading(connection, queue, deadline)?;
        if Instant::now() >= stop {
            return Ok(());
        }
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        match poll_fd(connection.as_fd().as_raw_fd(), libc::POLLIN, stop) {
            Ok(events) if events & libc::POLLIN != 0 => match guard.read() {
                Ok(_) => {}
                Err(wayland_client::backend::WaylandError::Io(e))
                    if e.kind() == std::io::ErrorKind::WouldBlock => {}
                Err(e) => return Err(e.to_string()),
            },
            Ok(events) => return Err(format!("observation socket error {events}")),
            Err(_) if Instant::now() >= stop => return Ok(()),
            Err(error) => return Err(error),
        }
    }
}
fn pointer_feedback(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut evidence = json!({"mode":"pointer-feedback","ok":false,"settle_ms":200,"observation_ms":400,"maximum_feedback_commits":32,"allowed_stationary_motions_after_seed":1});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        evidence["socket"] = json!(path);
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let mut state = State::new(start);
        state.pointer_feedback_enabled = true;
        connection.display().get_registry(&qh, ());
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pointer registry",
            |s| s.last_sync >= sync,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pointer seat binding",
            |s| s.last_sync >= sync,
        )?;
        if state.compositor.is_none()
            || state.wm.is_none()
            || state.shm.is_none()
            || state.pointer.is_none()
        {
            return Err("missing pointer/compositor/SHM/XDG fixture prerequisites".into());
        }
        let first = state.create(&qh, "stationary-pointer-feedback");
        state.windows[first].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pointer configure",
            |s| s.windows[first].configures >= 1,
        )?;
        state.map(first, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pointer enter and mapped frame",
            |s| s.pointer_on_primary && s.windows[first].frames >= 1,
        )?;
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(200),
            deadline,
        )?;
        if state.pointer_xy != Some((0.0, 0.0)) || !state.pointer_on_primary {
            return Err(format!(
                "initial pointer is not stationary at fixture origin: {:?}; run without host pointer interaction",
                state.pointer_xy
            ));
        }
        let ipc = std::env::var_os("FLUXWM_SOCKET")
            .ok_or("pointer-feedback requires FLUXWM_SOCKET diagnostics")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let before = diagnostics.snapshot(deadline)?;
        let baseline = (
            state.stationary_motions,
            state.changing_motions,
            state.pointer_enters,
            state.pointer_leaves,
        );
        evidence["before"] = before.clone();
        evidence["initial_pointer_xy"] = json!(state.pointer_xy);
        evidence["baseline_stationary_motions"] = json!(baseline.0);
        state.commit_on_motion = true;
        state.windows[first].surface.damage(0, 0, 1, 1);
        state.windows[first].surface.commit();
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(400),
            deadline,
        )?;
        state.commit_on_motion = false;
        let after = diagnostics.snapshot(deadline)?;
        let stationary = state.stationary_motions - baseline.0;
        evidence["after"] = after.clone();
        evidence["stationary_motions_after_seed"] = json!(stationary);
        evidence["feedback_commits"] = json!(state.feedback_commits);
        evidence["changing_motions_after_seed"] = json!(state.changing_motions - baseline.1);
        evidence["redraw_delta"] =
            json!(counter(&after, "redraws").saturating_sub(counter(&before, "redraws")));
        evidence["composition_delta"] =
            json!(counter(&after, "compositions").saturating_sub(counter(&before, "compositions")));
        evidence["events"] = json!(state.events);
        if state.changing_motions != baseline.1
            || state.pointer_enters != baseline.2
            || state.pointer_leaves != baseline.3
        {
            return Err(
                "pointer moved or focus changed during no-input interval; contaminated run".into(),
            );
        }
        if stationary > 1 {
            return Err(format!(
                "stationary pointer feedback: {stationary} repeated motions after one seed commit (maximum 1)"
            ));
        }
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let parsed = match args.as_slice() {
        [mode] if mode == "lifecycle" || mode == "slow-reader" || mode == "pointer-feedback" => {
            Ok((mode.as_str(), Duration::from_secs(5)))
        }
        [mode, flag, value]
            if (mode == "lifecycle" || mode == "slow-reader" || mode == "pointer-feedback")
                && flag == "--timeout-ms" =>
        {
            value
                .parse::<u64>()
                .ok()
                .filter(|n| (1..=5000).contains(n))
                .map(|n| (mode.as_str(), Duration::from_millis(n)))
                .ok_or("timeout must be 1..=5000 ms")
        }
        _ => Err(
            "usage: fluxwm-fixture lifecycle|slow-reader|pointer-feedback [--timeout-ms 1..5000]",
        ),
    };
    let result = match parsed {
        Ok(("slow-reader", timeout)) => slow_reader(timeout),
        Ok(("pointer-feedback", timeout)) => pointer_feedback(timeout),
        Ok((_, timeout)) => run(timeout),
        Err(error) => json!({"ok":false,"error":error}),
    };
    println!("{result}");
    if result.get("ok") != Some(&Json::Bool(true)) {
        std::process::exit(1);
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shm_pattern_is_native_xrgb_with_four_distinct_quadrants() {
        let image = pixels(0);
        let pixel =
            |x, y| &image[((y * WIDTH + x) * 4) as usize..((y * WIDTH + x) * 4 + 4) as usize];
        assert_eq!(image.len(), (WIDTH * HEIGHT * 4) as usize);
        assert_eq!(pixel(0, 0), 0xffff0000u32.to_ne_bytes());
        assert_eq!(pixel(WIDTH - 1, 0), 0xff00ff00u32.to_ne_bytes());
        assert_eq!(pixel(0, HEIGHT - 1), 0xff0000ffu32.to_ne_bytes());
        assert_eq!(pixel(WIDTH - 1, HEIGHT - 1), 0xffffff00u32.to_ne_bytes());
        assert!(
            pixels(1)
                .as_chunks::<4>()
                .0
                .iter()
                .all(|p| *p == 0xffff00ffu32.to_ne_bytes())
        );
    }
    #[test]
    fn a_silent_server_cannot_hold_the_event_loop_past_the_deadline() {
        let path =
            std::env::temp_dir().join(format!("fluxwm-fixture-silent-{}", std::process::id()));
        let listener = std::os::unix::net::UnixListener::bind(&path).unwrap();
        let start = Instant::now();
        let deadline = start + Duration::from_millis(30);
        let connection = connect(&path, deadline).unwrap();
        let (_server, _) = listener.accept().unwrap();
        let mut queue = connection.new_event_queue::<State>();
        let mut state = State::new(start);
        let sync = state.sync(&connection, &queue.handle());
        let result = until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "silent server",
            |s| s.last_sync >= sync,
        );
        assert!(result.unwrap_err().contains("deadline"));
        assert!(start.elapsed() < Duration::from_secs(1));
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn diagnostics_rejects_oversized_response_before_allocating_payload() {
        let (client, mut server) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        server.write_all(&65537_u32.to_be_bytes()).unwrap();
        let mut diagnostics = Diagnostics {
            socket: client,
            next_id: 0,
        };
        let error = diagnostics
            .snapshot(Instant::now() + Duration::from_millis(100))
            .unwrap_err();
        assert!(error.contains("outside 1..65536"));
    }
    #[test]
    fn diagnostics_silent_peer_cannot_exceed_deadline() {
        let (client, _server) = UnixStream::pair().unwrap();
        client.set_nonblocking(true).unwrap();
        let mut diagnostics = Diagnostics {
            socket: client,
            next_id: 0,
        };
        let start = Instant::now();
        let error = diagnostics
            .snapshot(start + Duration::from_millis(30))
            .unwrap_err();
        assert!(error.contains("deadline"));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn idle_observation_waits_for_its_interval_without_false_timeout() {
        let (client, _server) = UnixStream::pair().unwrap();
        let connection = Connection::from_socket(client).unwrap();
        let mut queue = connection.new_event_queue::<State>();
        let start = Instant::now();
        let mut state = State::new(start);
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(30),
            start + Duration::from_secs(1),
        )
        .unwrap();
        assert!(start.elapsed() >= Duration::from_millis(30));
        assert!(start.elapsed() < Duration::from_secs(1));
    }
    #[test]
    fn missing_socket_fails_without_blocking() {
        let deadline = Instant::now() + Duration::from_millis(100);
        assert!(connect(Path::new("/no-such-fluxwm-fixture-socket"), deadline).is_err());
    }
}
