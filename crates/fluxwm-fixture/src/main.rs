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
        wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_pointer, wl_registry, wl_seat,
        wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
    },
};
use wayland_protocols::xdg::shell::client::{
    xdg_popup, xdg_positioner, xdg_surface, xdg_toplevel, xdg_wm_base,
};

type Result<T> = std::result::Result<T, String>;
const WIDTH: i32 = 320;
const HEIGHT: i32 = 240;
#[derive(Clone, Copy, Debug)]
enum Callback {
    Sync(u32),
    Frame(usize),
    PopupFrame(usize),
    ClipChildFrame,
}
struct Window {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    toplevel: xdg_toplevel::XdgToplevel,
    buffer: Option<wl_buffer::WlBuffer>,
    _storage: Option<File>,
    configures: u32,
    ack_configures: bool,
    configure_serials: Vec<u32>,
    configure_sizes: Vec<[i32; 2]>,
    pending_size: [i32; 2],
    frames: u32,
    capabilities: u32,
    destroyed: bool,
}
#[derive(Clone, Copy, Debug)]
struct PopupTag(usize);
struct FixturePopup {
    surface: wl_surface::WlSurface,
    xdg: xdg_surface::XdgSurface,
    popup: xdg_popup::XdgPopup,
    buffer: Option<wl_buffer::WlBuffer>,
    storage: Option<File>,
    configures: u32,
    geometry_configures: u32,
    geometry: Option<[i32; 4]>,
    frames: u32,
    done: u32,
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
    popup_mode: bool,
    popups: Vec<FixturePopup>,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    keyboard_focus: Option<wl_surface::WlSurface>,
    popup_keyboard_enters: u32,
    container_mode: bool,
    popup_clip_mode: bool,
    input_probe_mode: bool,
    focus_popup_probe: bool,
    focus_popup_serial: Option<u32>,
    event_file: Option<File>,
    event_file_bytes: usize,
    subcompositor: Option<wl_subcompositor::WlSubcompositor>,
    clip_child: Option<wl_surface::WlSurface>,
    clip_child_frames: u32,
    clip_child_motions: u32,
    pointer_focus_surface: Option<wl_surface::WlSurface>,
    pointer_focus_xy: Option<(f64, f64)>,
    health_mode: bool,
    health_ignore: bool,
    health_serials: Vec<u32>,
    health_primary: Option<xdg_wm_base::XdgWmBase>,
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
            popup_mode: false,
            popups: Vec::new(),
            keyboard: None,
            keyboard_focus: None,
            popup_keyboard_enters: 0,
            container_mode: false,
            popup_clip_mode: false,
            input_probe_mode: false,
            focus_popup_probe: false,
            focus_popup_serial: None,
            event_file: None,
            event_file_bytes: 0,
            subcompositor: None,
            clip_child: None,
            clip_child_frames: 0,
            clip_child_motions: 0,
            pointer_focus_surface: None,
            pointer_focus_xy: None,
            health_mode: false,
            health_ignore: false,
            health_serials: Vec::new(),
            health_primary: None,
        }
    }
    fn record(&mut self, event: Json) {
        if self.events.len() >= 1024 {
            self.error = Some("event evidence limit exceeded".into());
            return;
        }
        let record = json!({"at_ms":self.start.elapsed().as_millis(),"event":event});
        if let Some(file) = self.event_file.as_mut() {
            let result = (|| -> std::io::Result<()> {
                let mut bytes = serde_json::to_vec(&record)?;
                bytes.push(b'\n');
                if self.event_file_bytes + bytes.len() > 1024 * 1024 {
                    return Err(std::io::Error::other("event file exceeds1MiB budget"));
                }
                file.write_all(&bytes)?;
                file.flush()?;
                self.event_file_bytes += bytes.len();
                Ok(())
            })();
            if let Err(error) = result {
                self.error = Some(format!("event stream write failed: {error}"));
            }
        }
        self.events.push(record);
    }
    fn pointer_root(&self) -> Option<usize> {
        self.pointer_focus_surface
            .as_ref()
            .and_then(|surface| self.windows.iter().position(|w| &w.surface == surface))
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
            ack_configures: true,
            configure_serials: Vec::new(),
            configure_sizes: Vec::new(),
            pending_size: [0, 0],
            frames: 0,
            capabilities: 0,
            destroyed: false,
        });
        self.record(json!({"create":n,"name":name}));
        n
    }
    fn create_popup(&mut self, parent: usize, invalid_grab: bool, qh: &QueueHandle<Self>) -> usize {
        self.create_popup_kind(parent, invalid_grab, false, qh)
    }
    fn create_popup_kind(
        &mut self,
        parent: usize,
        invalid_grab: bool,
        viewport_crossing: bool,
        qh: &QueueHandle<Self>,
    ) -> usize {
        let index = self.popups.len();
        let surface = self.compositor.as_ref().unwrap().create_surface(qh, ());
        let xdg = self
            .wm
            .as_ref()
            .unwrap()
            .get_xdg_surface(&surface, qh, PopupTag(index));
        let positioner = self.wm.as_ref().unwrap().create_positioner(qh, ());
        if viewport_crossing {
            positioner.set_size(80, 60);
            positioner.set_anchor_rect(0, 0, 1, 1);
            positioner.set_anchor(xdg_positioner::Anchor::TopLeft);
            positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
            positioner.set_offset(-64, 0);
            positioner.set_constraint_adjustment(
                xdg_positioner::ConstraintAdjustment::SlideX
                    | xdg_positioner::ConstraintAdjustment::SlideY,
            );
        } else {
            positioner.set_size(64, 48);
            positioner.set_anchor_rect(20, 20, 1, 1);
            positioner.set_anchor(xdg_positioner::Anchor::BottomRight);
            positioner.set_gravity(xdg_positioner::Gravity::BottomRight);
        }
        let popup = xdg.get_popup(
            Some(&self.windows[parent].xdg),
            &positioner,
            qh,
            PopupTag(index),
        );
        positioner.destroy();
        if invalid_grab {
            popup.grab(self.seat.as_ref().unwrap(), 0);
        }
        surface.commit();
        self.popups.push(FixturePopup {
            surface,
            xdg,
            popup,
            buffer: None,
            storage: None,
            configures: 0,
            geometry_configures: 0,
            geometry: None,
            frames: 0,
            done: 0,
            destroyed: false,
        });
        self.record(json!({"create_popup":index,"parent":parent,"invalid_grab_serial":if invalid_grab {Some(0)} else {None}}));
        index
    }
    fn map_popup(&mut self, index: usize, qh: &QueueHandle<Self>) -> Result<()> {
        self.map_popup_sized(index, 64, 48, qh)
    }
    fn map_popup_sized(
        &mut self,
        index: usize,
        width: i32,
        height: i32,
        qh: &QueueHandle<Self>,
    ) -> Result<()> {
        if !(1..=512).contains(&width) || !(1..=512).contains(&height) {
            return Err("popup buffer dimensions outside fixture bounds".into());
        }
        if self.popups[index].configures == 0 || self.popups[index].done > 0 {
            return Err("popup is not configured/live before mapping".into());
        }
        let mut file = memory_file()?;
        let bytes = 0xffff00ff_u32
            .to_ne_bytes()
            .repeat((width * height) as usize);
        file.set_len(bytes.len() as u64)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), bytes.len() as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            wl_shm::Format::Xrgb8888,
            qh,
            (),
        );
        pool.destroy();
        let p = &mut self.popups[index];
        p.xdg.set_window_geometry(0, 0, width, height);
        p.surface.attach(Some(&buffer), 0, 0);
        p.surface.damage(0, 0, width, height);
        p.surface.frame(qh, Callback::PopupFrame(index));
        p.surface.commit();
        p.buffer = Some(buffer);
        p.storage = Some(file);
        self.record(json!({"map_popup_commit":index}));
        Ok(())
    }
    fn destroy_popup(&mut self, index: usize) {
        let p = &mut self.popups[index];
        p.popup.destroy();
        p.xdg.destroy();
        p.surface.destroy();
        if let Some(buffer) = p.buffer.take() {
            buffer.destroy();
        }
        p.destroyed = true;
        self.record(json!({"destroy_popup":index}));
    }
    fn map_container_root(&mut self, root: usize, qh: &QueueHandle<Self>) -> Result<()> {
        let mut file = memory_file()?;
        let bytes = container_pixels();
        file.set_len(bytes.len() as u64)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), bytes.len() as i32, qh, ());
        let buffer = pool.create_buffer(0, 1900, 240, 1900 * 4, wl_shm::Format::Xrgb8888, qh, ());
        pool.destroy();
        let w = &mut self.windows[root];
        w.xdg.set_window_geometry(0, 0, 1900, 240);
        w.surface.attach(Some(&buffer), 0, 0);
        w.surface.damage(0, 0, 1900, 240);
        w.surface.frame(qh, Callback::Frame(root));
        w.surface.commit();
        w.buffer = Some(buffer);
        w._storage = Some(file);
        Ok(())
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
                "wl_subcompositor" if state.container_mode && state.subcompositor.is_none() => {
                    state.subcompositor = Some(registry.bind(name, 1, qh, ()));
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
        state: &mut Self,
        wm: &xdg_wm_base::XdgWmBase,
        event: xdg_wm_base::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let xdg_wm_base::Event::Ping { serial } = event {
            let primary = state.health_primary.as_ref().or(state.wm.as_ref()) == Some(wm);
            if state.health_mode {
                state.record(
                    json!({"ping":serial,"primary":primary,"wm_object":wm.id().protocol_id()}),
                );
                if primary {
                    if state.health_serials.len() >= 1024 {
                        state.error = Some("ping evidence limit exceeded".into());
                        return;
                    }
                    state.health_serials.push(serial);
                }
            }
            if !(state.health_ignore && primary) {
                wm.pong(serial);
                if state.health_mode {
                    state.record(json!({"pong_sent":serial,"primary":primary}));
                }
            }
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
            let window = &mut state.windows[*index];
            if window.configure_serials.len() >= 512 {
                state.error = Some("toplevel configure evidence limit exceeded".into());
                return;
            }
            let size = window.pending_size;
            window.configure_serials.push(serial);
            window.configure_sizes.push(size);
            window.configures += 1;
            if window.ack_configures {
                surface.ack_configure(serial);
                state.record(json!({"configure_ack":index,"serial":serial,"size":size}));
            } else {
                state.record(json!({"configure_held":index,"serial":serial,"size":size}));
            }
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
            xdg_toplevel::Event::Configure { width, height, .. } => {
                state.windows[*index].pending_size = [width, height];
            }
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
                Callback::ClipChildFrame => {
                    state.clip_child_frames += 1;
                    state.record(json!({"clip_child_frame":true}));
                }
                Callback::PopupFrame(n) => {
                    state.popups[*n].frames += 1;
                    state.record(json!({"popup_frame_done":n,"timestamp":callback_data}));
                    if state.focus_popup_probe {
                        state.record(json!({"type":"popup.frame","popup":n}));
                    }
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
        {
            if state.pointer_feedback_enabled
                && state.pointer.is_none()
                && capabilities.contains(wl_seat::Capability::Pointer)
            {
                state.pointer = Some(seat.get_pointer(qh, ()));
            }
            if state.popup_mode
                && state.keyboard.is_none()
                && capabilities.contains(wl_seat::Capability::Keyboard)
            {
                state.keyboard = Some(seat.get_keyboard(qh, ()));
            }
        }
    }
}
impl Dispatch<wl_keyboard::WlKeyboard, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_keyboard::WlKeyboard,
        event: wl_keyboard::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_keyboard::Event::Enter {
                surface,
                keys,
                serial,
            } => {
                if state.input_probe_mode {
                    if keys.len() > 1024 || keys.len() % 4 != 0 {
                        state.error = Some("invalid keyboard enter key array".into());
                        return;
                    }
                    let keys: Vec<_> = keys
                        .as_chunks::<4>()
                        .0
                        .iter()
                        .copied()
                        .map(u32::from_ne_bytes)
                        .collect();
                    state.record(json!({"type":"keyboard.enter","root":state.windows.iter().position(|w|w.surface==surface),"popup":state.popups.iter().position(|p|p.surface==surface),"keys":keys,"serial":serial}));
                }
                let popup = state.popups.iter().position(|p| p.surface == surface);
                if popup.is_some() {
                    state.popup_keyboard_enters += 1;
                }
                state.keyboard_focus = Some(surface.clone());
                state.record(json!({"keyboard_enter_window":state.windows.iter().position(|w|w.surface==surface),"keyboard_enter_popup":popup}));
            }
            wl_keyboard::Event::Leave { serial, .. } => {
                if state.input_probe_mode {
                    state.record(json!({"type":"keyboard.leave","serial":serial}));
                }
                state.keyboard_focus = None;
                state.record(json!({"keyboard_leave":true}));
            }
            wl_keyboard::Event::Key {
                serial,
                time,
                key,
                state: key_state,
            } if state.input_probe_mode => {
                state.record(json!({"type":"keyboard.key","root":state.keyboard_focus.as_ref().and_then(|surface|state.windows.iter().position(|w|&w.surface==surface)),"popup":state.keyboard_focus.as_ref().and_then(|surface|state.popups.iter().position(|p|&p.surface==surface)),"key":key,"state":match key_state { WEnum::Value(wl_keyboard::KeyState::Pressed)=>"pressed",WEnum::Value(wl_keyboard::KeyState::Released)=>"released",_=>"unknown" },"serial":serial,"time":time}));
                if state.focus_popup_probe
                    && key == 31
                    && key_state == WEnum::Value(wl_keyboard::KeyState::Pressed)
                    && state.popups.len() == 1
                    && state.popups[0].done > 0
                {
                    let serial = state.focus_popup_serial.unwrap();
                    state.destroy_popup(0);
                    let popup = state.create_popup(0, false, qh);
                    state.popups[popup]
                        .popup
                        .grab(state.seat.as_ref().unwrap(), serial);
                    // The pinned helper applies requested_grab at surface commit.
                    state.popups[popup].surface.commit();
                    state.record(
                        json!({"type":"popup.stale_grab_requested","popup":popup,"serial":serial}),
                    );
                }
            }
            wl_keyboard::Event::Modifiers {
                serial,
                mods_depressed,
                mods_latched,
                mods_locked,
                group,
            } if state.input_probe_mode => {
                state.record(json!({"type":"keyboard.modifiers","serial":serial,"depressed":mods_depressed,"latched":mods_latched,"locked":mods_locked,"group":group}));
            }
            _ => {}
        }
    }
}
impl Dispatch<xdg_surface::XdgSurface, PopupTag> for State {
    fn event(
        state: &mut Self,
        surface: &xdg_surface::XdgSurface,
        event: xdg_surface::Event,
        index: &PopupTag,
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if let xdg_surface::Event::Configure { serial } = event {
            surface.ack_configure(serial);
            state.popups[index.0].configures += 1;
            state.record(json!({"popup_configure_ack":index.0,"serial":serial}));
            if state.focus_popup_probe
                && index.0 != 1
                && let Err(error) = state.map_popup(index.0, qh)
            {
                state.error = Some(error);
            }
        }
    }
}
impl Dispatch<xdg_popup::XdgPopup, PopupTag> for State {
    fn event(
        state: &mut Self,
        _: &xdg_popup::XdgPopup,
        event: xdg_popup::Event,
        index: &PopupTag,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            xdg_popup::Event::Configure {
                x,
                y,
                width,
                height,
            } => {
                state.popups[index.0].geometry_configures += 1;
                state.popups[index.0].geometry = Some([x, y, width, height]);
                state.record(json!({"popup_geometry":index.0,"rect":[x,y,width,height]}));
            }
            xdg_popup::Event::PopupDone => {
                state.popups[index.0].done += 1;
                state.record(json!({"popup_done":index.0}));
                if state.focus_popup_probe {
                    state.record(json!({"type":"popup.done","popup":index.0}));
                }
            }
            _ => {}
        }
    }
}
delegate_noop!(State:ignore xdg_positioner::XdgPositioner);
delegate_noop!(State:ignore wl_subcompositor::WlSubcompositor);
delegate_noop!(State:ignore wl_subsurface::WlSubsurface);
fn pointer_axis_name(axis: WEnum<wl_pointer::Axis>) -> &'static str {
    match axis {
        WEnum::Value(wl_pointer::Axis::VerticalScroll) => "vertical",
        WEnum::Value(wl_pointer::Axis::HorizontalScroll) => "horizontal",
        _ => "unknown",
    }
}
impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        if state.input_probe_mode {
            let root = state.pointer_root();
            let popup = state
                .pointer_focus_surface
                .as_ref()
                .and_then(|surface| state.popups.iter().position(|p| &p.surface == surface));
            match &event {
                wl_pointer::Event::Enter {serial,surface,surface_x,surface_y} => {let root=state.windows.iter().position(|w|w.surface==*surface);state.record(json!({"type":"pointer.enter","root":root,"popup":state.popups.iter().position(|p|p.surface==*surface),"serial":serial,"x":surface_x,"y":surface_y}));}
                wl_pointer::Event::Leave {serial,surface} => {let root=state.windows.iter().position(|w|w.surface==*surface);state.record(json!({"type":"pointer.leave","root":root,"serial":serial}));}
                wl_pointer::Event::Motion {time,surface_x,surface_y} => state.record(json!({"type":"pointer.motion","root":root,"time":time,"x":surface_x,"y":surface_y})),
                wl_pointer::Event::Button {serial,time,button,state:button_state} => {
                    let button_state=match button_state {WEnum::Value(wl_pointer::ButtonState::Pressed)=>"pressed",WEnum::Value(wl_pointer::ButtonState::Released)=>"released",_=>{state.error=Some("unknown pointer button state".into());"unknown"}};
                    state.record(json!({"type":"pointer.button","root":root,"popup":popup,"serial":serial,"time":time,"button":button,"state":button_state}));
                    if state.focus_popup_probe && (state.popups.is_empty() || (state.popups.len() == 2 && state.popups.iter().all(|p| p.done > 0))) && root == Some(0) && *button == 272 && button_state == "pressed" {
                        for index in 0..state.popups.len() { if !state.popups[index].destroyed { state.destroy_popup(index); } }
                        state.focus_popup_serial = Some(*serial);
                        let popup = state.create_popup(0, false, qh);
                        state.popups[popup].popup.grab(state.seat.as_ref().unwrap(), *serial);
                        state.record(json!({"type":"popup.grab_requested","popup":popup,"serial":serial}));
                    }
                }
                wl_pointer::Event::Axis {time,axis,value} => {let axis=pointer_axis_name(*axis);state.record(json!({"type":"pointer.axis","root":root,"time":time,"axis":axis,"value":value}));}
                wl_pointer::Event::AxisStop {time,axis}=>state.record(json!({"type":"pointer.axis_stop","root":root,"time":time,"axis":pointer_axis_name(*axis)})),
                wl_pointer::Event::Frame=>state.record(json!({"type":"pointer.frame","root":root})),
                _=>{}
            }
        }
        match &event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_focus_surface = Some(surface.clone());
                state.pointer_focus_xy = Some((*surface_x, *surface_y));
                if state.popup_clip_mode {
                    state.record(json!({"popup_clip_pointer_enter":state.popups.iter().position(|p|p.surface==*surface),"xy":[surface_x,surface_y]}));
                }
                if state.container_mode {
                    state.record(json!({"clip_pointer_enter_child":state.clip_child.as_ref()==Some(surface),"xy":[surface_x,surface_y]}));
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if state.container_mode {
                    state.record(json!({"clip_pointer_leave_child":state.pointer_focus_surface==state.clip_child}));
                }
                state.pointer_focus_surface = None;
                state.pointer_focus_xy = None;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_focus_xy = Some((*surface_x, *surface_y));
                if state.clip_child.is_some() && state.pointer_focus_surface == state.clip_child {
                    state.clip_child_motions += 1;
                }
            }
            _ => {}
        }
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
    // This fixture requires all four 160x120 quadrants to be visible. A host
    // tiled to less than 640 px can otherwise assign a half-width container below
    // our 320 px buffer and correctly clip it. Declare the actual pixel workload.
    state.windows[first].toplevel.set_min_size(320, 240);
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
    last_sequence: Option<u64>,
}
impl Diagnostics {
    fn new(path: &Path, deadline: Instant) -> Result<Self> {
        Ok(Self {
            socket: connect_stream(path, deadline)?,
            next_id: 0,
            last_sequence: None,
        })
    }
    fn call(&mut self, method: &str, params: Json, deadline: Instant) -> Result<Json> {
        let id = format!("fixture-{}", self.next_id);
        self.next_id = self
            .next_id
            .checked_add(1)
            .ok_or("diagnostic request identity capacity exhausted")?;
        let payload =
            serde_json::to_vec(&json!({"version":1,"id":id,"method":method,"params":params}))
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
        let sequence = response
            .get("state_sequence")
            .and_then(Json::as_str)
            .and_then(|s| s.parse::<u64>().ok())
            .ok_or("missing string state sequence")?;
        if self.last_sequence.is_some_and(|old| sequence < old) {
            return Err("IPC sequence regressed".into());
        }
        self.last_sequence = Some(sequence);
        response
            .get("result")
            .cloned()
            .ok_or_else(|| format!("IPC {method} failed: {response}"))
    }
    fn snapshot(&mut self, deadline: Instant) -> Result<Json> {
        let result = self.call("diagnostics", json!({}), deadline)?;
        let stats = result
            .get("runtime")
            .cloned()
            .ok_or_else(|| format!("missing diagnostic runtime counters: {result}"))?;
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
fn popup_lifecycle(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.popup_mode = true;
    let mut evidence = json!({"mode":"popup-lifecycle","ok":false,"popup_size":[64,48],"physical_input_injected":false});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        evidence["socket"] = json!(path);
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup registry",
            |s| s.last_sync >= sync,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup seat binding",
            |s| s.last_sync >= sync,
        )?;
        if state.compositor.is_none()
            || state.wm.is_none()
            || state.shm.is_none()
            || state.seat.is_none()
            || state.keyboard.is_none()
        {
            return Err("popup fixture prerequisites missing".into());
        }
        let root = state.create(&qh, "popup-parent");
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup root configure",
            |s| s.windows[root].configures >= 1,
        )?;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup root map",
            |s| s.windows[root].frames >= 1,
        )?;
        let popup = state.create_popup(root, false, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "plain popup configure",
            |s| s.popups[popup].configures >= 1,
        )?;
        state.map_popup(popup, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "plain popup frame",
            |s| s.popups[popup].frames >= 1,
        )?;
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "root unmap processed",
            |s| s.last_sync >= sync,
        )?;
        evidence["root_null_unmap_processed"] = json!(true);
        evidence["stage"] = json!("root-null-unmap-popup-done");
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline.min(Instant::now() + Duration::from_millis(400)),
            "root-unmap popup_done",
            |s| s.popups[popup].done >= 1,
        )?;
        evidence["non_grabbing_popup_dismissed_on_unmap"] = json!(true);
        state.destroy_popup(popup);
        let prior = state.windows[root].configures;
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup root remap configure",
            |s| s.windows[root].configures > prior,
        )?;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup root remapped",
            |s| s.windows[root].frames >= 2,
        )?;
        let foreground = state.create(&qh, "foreground-focus-owner");
        state.windows[foreground].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "foreground configure",
            |s| s.windows[foreground].configures >= 1,
        )?;
        state.map(foreground, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "foreground focus",
            |s| {
                s.windows[foreground].frames >= 1
                    && s.keyboard_focus.as_ref() == Some(&s.windows[foreground].surface)
            },
        )?;
        let popup_enters = state.popup_keyboard_enters;
        let invalid = state.create_popup(root, true, &qh);
        evidence["stage"] = json!("invalid-background-popup-grab");
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "invalid popup grab dismissal",
            |s| s.popups[invalid].done >= 1,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "invalid popup focus barrier",
            |s| s.last_sync >= sync,
        )?;
        if state.keyboard_focus.as_ref() != Some(&state.windows[foreground].surface)
            || state.popup_keyboard_enters != popup_enters
        {
            return Err("invalid background popup stole keyboard focus".into());
        }
        evidence["invalid_serial_zero_popup_dismissed"] = json!(true);
        evidence["background_keyboard_focus_preserved"] = json!(true);
        state.destroy_popup(invalid);
        let last = state.create_popup(foreground, false, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "destroy-case popup configure",
            |s| s.popups[last].configures >= 1,
        )?;
        state.map_popup(last, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "destroy-case popup frame",
            |s| s.popups[last].frames >= 1,
        )?;
        state.windows[foreground].toplevel.destroy();
        evidence["stage"] = json!("root-role-destroy-popup-done");
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "destroyed-root popup_done",
            |s| s.popups[last].done >= 1,
        )?;
        evidence["popup_dismissed_on_root_role_destroy"] = json!(true);
        state.destroy_popup(last);
        state.windows[foreground].xdg.destroy();
        state.windows[foreground].surface.destroy();
        state.windows[foreground].destroyed = true;
        state.windows[root].toplevel.destroy();
        state.windows[root].xdg.destroy();
        state.windows[root].surface.destroy();
        state.windows[root].destroyed = true;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup cleanup barrier",
            |s| s.last_sync >= sync,
        )?;
        let ipc = std::env::var_os("FLUXWM_SOCKET")
            .ok_or("popup-lifecycle requires FLUXWM_SOCKET diagnostics")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let stats = diagnostics.snapshot(deadline)?;
        if counter(&stats, "maps") != 3
            || counter(&stats, "unmaps") != 1
            || counter(&stats, "destroyed") != 2
        {
            return Err(format!("unexpected popup-root cleanup counters: {stats}"));
        }
        evidence["final_runtime"] = stats;
        evidence["stage"] = json!("complete");
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence["popup_keyboard_enters"] = json!(state.popup_keyboard_enters);
    evidence["popups"]=json!(state.popups.iter().enumerate().map(|(i,p)|json!({"index":i,"configure_acks":p.configures,"geometry_configures":p.geometry_configures,"frames":p.frames,"popup_done":p.done,"destroyed":p.destroyed})).collect::<Vec<_>>());
    evidence
}
fn container_pixels() -> Vec<u8> {
    let mut bytes = Vec::with_capacity(1900 * 240 * 4);
    for y in 0..240 {
        for x in 0..1900 {
            let pixel = if x >= 320 {
                0xff00ffff_u32
            } else {
                match (x < 160, y < 120) {
                    (true, true) => 0xffff0000,
                    (false, true) => 0xff00ff00,
                    (true, false) => 0xff0000ff,
                    (false, false) => 0xffffff00,
                }
            };
            bytes.extend_from_slice(&pixel.to_ne_bytes());
        }
    }
    bytes
}
fn container_clip(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.container_mode = true;
    state.pointer_feedback_enabled = true;
    let mut evidence = json!({"mode":"container-clip","ok":false,"actual_shm_size":[1900,240],"requested_container_width":280,"settle_ms":1000,"physical_input_injected":false,"pixel_proof_requires_delayed_compositor_screenshot":true,
        "expected_visible_content_rect":{"x":0,"y":0,"width":280,"height":240},
        "expected_rgb_counts":{"red":19200,"green":14400,"blue":19200,"yellow":14400,"cyan":0},
        "expected_cyan_rgb":[0,255,255],"expected_right_clip_x":280});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        evidence["socket"] = json!(path);
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "container registry",
            |s| s.last_sync >= sync,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "container global binding",
            |s| s.last_sync >= sync,
        )?;
        if state.compositor.is_none() || state.wm.is_none() || state.shm.is_none() {
            return Err("container fixture globals missing".into());
        }
        let ipc =
            std::env::var_os("FLUXWM_SOCKET").ok_or("container-clip requires FLUXWM_SOCKET")?;
        let mut control = Diagnostics::new(Path::new(&ipc), deadline)?;
        let outputs = control.call("outputs.list", json!({}), deadline)?;
        let output = outputs
            .pointer("/outputs/0/geometry")
            .ok_or("missing nested output geometry")?;
        if output["width"].as_f64().unwrap_or(0.0) < 320.0
            || output["height"].as_f64().unwrap_or(0.0) < 240.0
        {
            return Err("output must fit the fixture's expected 320x240 color region".into());
        }
        evidence["output_geometry"] = output.clone();
        let root = state.create(&qh, "oversized-container-clip");
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "container root configure",
            |s| s.windows[root].configures >= 1,
        )?;
        state.map_container_root(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "oversized mapped frame",
            |s| s.windows[root].frames >= 1,
        )?;
        control.call(
            "action",
            json!({"name":"workspace-mode","mode":"canvas"}),
            deadline,
        )?;
        let workspace = control.call("workspaces.list", json!({}), deadline)?;
        if workspace
            .pointer("/workspaces/0/mode")
            .and_then(Json::as_str)
            != Some("canvas")
        {
            return Err(format!("workspace did not enter canvas: {workspace}"));
        }
        evidence["workspace_mode"] = json!("canvas");
        let before = control.call("windows.list", json!({}), deadline)?;
        let windows = before["windows"]
            .as_array()
            .ok_or("windows.list missing array")?;
        if windows.len() != 1 {
            return Err("container-clip requires exactly one isolated toplevel".into());
        }
        let width = windows[0]["geometry"]["width"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("missing finite layout width")?;
        evidence["before_resize"] = before.clone();
        control.call(
            "action",
            json!({"name":"resize","delta":280.0-width}),
            deadline,
        )?;
        let after = control.call("windows.list", json!({}), deadline)?;
        let geometry = after
            .pointer("/windows/0/geometry")
            .ok_or("missing resized target")?;
        if geometry["width"].as_f64() != Some(280.0)
            || geometry["x"].as_f64() != Some(0.0)
            || geometry["y"].as_f64() != Some(0.0)
        {
            return Err(format!("unexpected container geometry: {geometry}"));
        }
        evidence["after_resize"] = after;
        evidence["resize_completed_at_ms"] = json!(start.elapsed().as_millis());
        // Acknowledge new configures but deliberately retain the original1900px
        // buffer and window geometry throughout the screenshot interval.
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(1000),
            deadline,
        )?;
        evidence["after_settle"] = control.call("windows.list", json!({}), deadline)?;
        evidence["runtime"] = control.snapshot(deadline)?;
        if evidence["after_settle"]
            .pointer("/windows/0/geometry/width")
            .and_then(Json::as_f64)
            != Some(280.0)
        {
            return Err("container width changed before screenshot interval ended".into());
        }
        // Delayed screenshot has completed before this separate input stage.
        // Keep explicit root window geometry so a negative subsurface cannot
        // silently shift the root's XDG geometry/scene origin.
        if state.pointer_focus_surface.as_ref() != Some(&state.windows[root].surface)
            || state.pointer_focus_xy != Some((0.0, 0.0))
        {
            return Err("container input stage requires stationary pointer at root(0,0)".into());
        }
        let child = state.compositor.as_ref().unwrap().create_surface(&qh, ());
        let subsurface = state
            .subcompositor
            .as_ref()
            .ok_or("missing wl_subcompositor for input clip stage")?
            .get_subsurface(&child, &state.windows[root].surface, &qh, ());
        subsurface.set_position(-64, 0);
        subsurface.place_above(&state.windows[root].surface);
        let mut child_storage = memory_file()?;
        let child_bytes = 0xffff00ff_u32.to_ne_bytes().repeat(80 * 80);
        child_storage
            .set_len(child_bytes.len() as u64)
            .map_err(|e| e.to_string())?;
        child_storage
            .write_all(&child_bytes)
            .map_err(|e| e.to_string())?;
        let child_pool = state.shm.as_ref().unwrap().create_pool(
            child_storage.as_fd(),
            child_bytes.len() as i32,
            &qh,
            (),
        );
        let child_buffer =
            child_pool.create_buffer(0, 80, 80, 80 * 4, wl_shm::Format::Xrgb8888, &qh, ());
        child_pool.destroy();
        state.clip_child = Some(child.clone());
        child.attach(Some(&child_buffer), 0, 0);
        child.damage(0, 0, 80, 80);
        child.frame(&qh, Callback::ClipChildFrame);
        child.commit();
        state.windows[root].xdg.set_window_geometry(0, 0, 1900, 240);
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "clipped child enter/frame",
            |s| s.clip_child_frames >= 1 && s.pointer_focus_surface == s.clip_child,
        )?;
        if state.pointer_focus_xy != Some((64.0, 0.0)) {
            return Err(format!(
                "child pointer entry is not stationary world origin: {:?}",
                state.pointer_focus_xy
            ));
        }
        evidence["child_enter_xy"] = json!(state.pointer_focus_xy);
        control.call(
            "action",
            json!({"name":"move","direction":"right"}),
            deadline,
        )?;
        let moved = control.call("windows.list", json!({}), deadline)?;
        if moved
            .pointer("/windows/0/geometry/x")
            .and_then(Json::as_f64)
            != Some(32.0)
        {
            return Err(format!("unexpected shifted container: {moved}"));
        }
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline.min(Instant::now() + Duration::from_millis(400)),
            "outside-container child pointer leave",
            |s| s.pointer_focus_surface.is_none(),
        )?;
        if state.clip_child_motions != 0 {
            return Err("pointer changed within child during clipped move; run is contaminated or clip routing is inconsistent".into());
        }
        evidence["input_clip_proof"] = json!({"ok":true,"stationary_world_pointer":[0,0],"child_before_world_rect":{"x":-64,"y":0,"width":80,"height":80},"child_after_world_rect":{"x":-32,"y":0,"width":80,"height":80},"container_after_world_rect":{"x":32,"y":0,"width":280},"pointer_still_inside_actual_child":true,"pointer_outside_container":true,"pointer_focus_after_move":null,"child_motion_events":state.clip_child_motions});
        evidence["after_input_move"] = moved;
        subsurface.destroy();
        child.destroy();
        child_buffer.destroy();
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence
}
fn verify_container_ppm(path: &Path) -> Json {
    let mut evidence = json!({"mode":"verify-container-clip","ok":false,"path":path});
    let result = (|| -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NONBLOCK | libc::O_CLOEXEC)
            .open(path)
            .map_err(|e| e.to_string())?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("PPM must be a regular file".into());
        }
        const LIMIT: u64 = 64 * 1024 * 1024;
        let mut data = Vec::new();
        file.take(LIMIT + 1)
            .read_to_end(&mut data)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 > LIMIT {
            return Err("PPM exceeds64MiB limit".into());
        }
        let mut parts = data.splitn(4, |b| *b == b'\n');
        let magic = parts.next().ok_or("missing PPM magic")?;
        let dimensions = parts.next().ok_or("missing PPM dimensions")?;
        let depth = parts.next().ok_or("missing PPM depth")?;
        let pixels = parts.next().ok_or("missing PPM pixels")?;
        if magic != b"P6" || depth != b"255" || dimensions.len() > 64 {
            return Err("expected bounded P6/8-bit header".into());
        }
        let mut dims = std::str::from_utf8(dimensions)
            .map_err(|e| e.to_string())?
            .split_whitespace();
        let width: usize = dims
            .next()
            .ok_or("missing width")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        let height: usize = dims
            .next()
            .ok_or("missing height")?
            .parse::<usize>()
            .map_err(|e| e.to_string())?;
        if !(320..=8192).contains(&width)
            || !(240..=8192).contains(&height)
            || dims.next().is_some()
        {
            return Err("PPM dimensions outside320x240..8192x8192".into());
        }
        if pixels.len()
            != width
                .checked_mul(height)
                .and_then(|v| v.checked_mul(3))
                .ok_or("PPM dimension overflow")?
        {
            return Err("PPM payload length mismatch".into());
        }
        let mut counts = [0_usize; 5];
        let mut bounds = [[width, height, 0, 0]; 5];
        for (i, pixel) in pixels.as_chunks::<3>().0.iter().enumerate() {
            let color = match pixel {
                [255, 0, 0] => Some(0),
                [0, 255, 0] => Some(1),
                [0, 0, 255] => Some(2),
                [255, 255, 0] => Some(3),
                [0, 255, 255] => Some(4),
                _ => None,
            };
            if let Some(c) = color {
                let (x, y) = (i % width, i / width);
                counts[c] += 1;
                bounds[c] = [
                    bounds[c][0].min(x),
                    bounds[c][1].min(y),
                    bounds[c][2].max(x),
                    bounds[c][3].max(y),
                ];
            }
        }
        evidence["width"] = json!(width);
        evidence["height"] = json!(height);
        evidence["rgb_counts"] = json!({"red":counts[0],"green":counts[1],"blue":counts[2],"yellow":counts[3],"cyan":counts[4]});
        evidence["bounds_rgb_yellow"] = json!(&bounds[..4]);
        if counts != [19200, 14400, 19200, 14400, 0] {
            return Err(format!("unexpected clipped RGB counts: {counts:?}"));
        }
        if bounds[..4]
            != [
                [0, 0, 159, 119],
                [160, 0, 279, 119],
                [0, 120, 159, 239],
                [160, 120, 279, 239],
            ]
        {
            return Err(format!(
                "unexpected clipped pixel bounds: {:?}",
                &bounds[..4]
            ));
        }
        let background = &pixels[pixels.len() - 3..];
        if (0..240).any(|y| &pixels[(y * width + 280) * 3..(y * width + 281) * 3] != background) {
            return Err("rightclip x280 does not match background".into());
        }
        evidence["background_rgb"] = json!(background);
        evidence["right_edge_x280_matches_background"] = json!(true);
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence
}
fn popup_clip(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.pointer_feedback_enabled = true;
    state.popup_clip_mode = true;
    let mut evidence = json!({"mode":"popup-clip","ok":false,"actual_shm_size":[1900,240],"settle_ms":1000,"physical_input_injected":false,"popup_request_offset":[-64,0],"popup_requested_size":[80,60],"expected_rgb_counts":{"magenta":4800,"red":16320,"green":14400,"blue":19200,"yellow":14400,"cyan":0},"expected_image_regions":{"popup":{"x":0,"y":0,"width":80,"height":60},"root_container":{"x":32,"y":0,"width":280},"root_visible_content":{"x":32,"y":0,"width":280,"height":240}},"pixel_proof_requires_delayed_compositor_screenshot":true});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        evidence["socket"] = json!(path);
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip registry",
            |s| s.last_sync >= sync,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip bound globals",
            |s| s.last_sync >= sync,
        )?;
        if state.compositor.is_none()
            || state.shm.is_none()
            || state.wm.is_none()
            || state.pointer.is_none()
        {
            return Err("popupclip prerequisites missing".into());
        }
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("popupclip requires FLUXWM_SOCKET")?;
        let mut control = Diagnostics::new(Path::new(&ipc), deadline)?;
        let outputs = control.call("outputs.list", json!({}), deadline)?;
        let output = outputs
            .pointer("/outputs/0/geometry")
            .ok_or("missing output geometry")?;
        if output["width"].as_f64().unwrap_or(0.0) < 320.0
            || output["height"].as_f64().unwrap_or(0.0) < 240.0
        {
            return Err("popupclip requires output at least320x240".into());
        }
        evidence["output_geometry"] = output.clone();
        let root = state.create(&qh, "popup-clip-owner");
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip root configure",
            |s| s.windows[root].configures >= 1,
        )?;
        state.map_container_root(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip root frame/pointer",
            |s| {
                s.windows[root].frames >= 1
                    && s.pointer_focus_surface.as_ref() == Some(&s.windows[root].surface)
            },
        )?;
        if state.pointer_focus_xy != Some((0.0, 0.0)) {
            return Err("popupclip requires stationary root pointer at0,0".into());
        }
        evidence["initial_root_pointer_xy"] = json!(state.pointer_focus_xy);
        control.call(
            "action",
            json!({"name":"workspace-mode","mode":"canvas"}),
            deadline,
        )?;
        let workspace = control.call("workspaces.list", json!({}), deadline)?;
        if workspace
            .pointer("/workspaces/0/mode")
            .and_then(Json::as_str)
            != Some("canvas")
        {
            return Err("workspace did not enter canvas".into());
        }
        evidence["workspace_mode"] = json!("canvas");
        let before = control.call("windows.list", json!({}), deadline)?;
        let windows = before["windows"].as_array().ok_or("missing root list")?;
        if windows.len() != 1 {
            return Err("popupclip requires one isolated root".into());
        }
        let width = windows[0]["geometry"]["width"]
            .as_f64()
            .filter(|v| v.is_finite())
            .ok_or("missing root width")?;
        control.call(
            "action",
            json!({"name":"resize","delta":280.0-width}),
            deadline,
        )?;
        control.call(
            "action",
            json!({"name":"move","direction":"right"}),
            deadline,
        )?;
        let moved = control.call("windows.list", json!({}), deadline)?;
        let geometry = moved
            .pointer("/windows/0/geometry")
            .ok_or("missing moved root")?;
        if geometry["x"].as_f64() != Some(32.0)
            || geometry["y"].as_f64() != Some(0.0)
            || geometry["width"].as_f64() != Some(280.0)
        {
            return Err(format!("unexpected popup owner geometry: {geometry}"));
        }
        evidence["root_after_move"] = moved;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pointer outside moved root",
            |s| s.pointer_focus_surface.is_none(),
        )?;
        evidence["pointer_outside_root_before_popup"] = json!(true);
        let popup = state.create_popup_kind(root, false, true, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip popup configure",
            |s| s.popups[popup].configures >= 1,
        )?;
        let actual = state.popups[popup]
            .geometry
            .ok_or("missing actual popup geometry event")?;
        evidence["actual_popup_configure"] = json!(actual);
        let world = [32 + actual[0], actual[1], actual[2], actual[3]];
        evidence["actual_popup_world_rect"] =
            json!({"x":world[0],"y":world[1],"width":world[2],"height":world[3]});
        if actual != [-32, 0, 80, 60] || world != [0, 0, 80, 60] {
            return Err(format!(
                "popup did not constrain to required viewport-crossing geometry: {actual:?}"
            ));
        }
        state.map_popup_sized(popup, 80, 60, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popupclip actual popup frame/enter",
            |s| {
                s.popups[popup].frames >= 1
                    && s.pointer_focus_surface.as_ref() == Some(&s.popups[popup].surface)
            },
        )?;
        if state.pointer_focus_xy != Some((0.0, 0.0)) {
            return Err("popupclip pointer entry was not at world origin".into());
        }
        evidence["popup_pointer_enter_xy"] = json!(state.pointer_focus_xy);
        evidence["popup_receives_pointer_outside_owner_clip"] = json!(true);
        evidence["popup_ready_at_ms"] = json!(start.elapsed().as_millis());
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(1000),
            deadline,
        )?;
        if state.pointer_focus_surface.as_ref() != Some(&state.popups[popup].surface)
            || state.pointer_focus_xy != Some((0.0, 0.0))
            || state.popups[popup].done != 0
            || state.popups[popup].geometry != Some(actual)
        {
            return Err("popup/focus/geometry changed during screenshot interval; contaminated or invalid scene".into());
        }
        evidence["after_settle"] = control.call("windows.list", json!({}), deadline)?;
        evidence["popup_frame_callbacks"] = json!(state.popups[popup].frames);
        evidence["popup_done_before_cleanup"] = json!(state.popups[popup].done);
        state.destroy_popup(popup);
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence
}
fn input_probe(timeout: Duration) -> Json {
    const OBSERVE_MS: u64 = 7000;
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.pointer_feedback_enabled = true;
    state.input_probe_mode = true;
    state.popup_mode = true; // Bind keyboard as well as pointer for the live journal.
    state.focus_popup_probe =
        std::env::var("FLUXWM_FIXTURE_GRAB_POPUP").ok().as_deref() == Some("1");
    let mut evidence = json!({"mode":"input-probe","ok":false,"observation_ms":OBSERVE_MS,"buffer_size":[320,240],"physical_input_injected_by_fixture":false});
    let result = (|| -> Result<()> {
        use std::os::unix::fs::OpenOptionsExt;
        let path = std::env::var_os("FLUXWM_FIXTURE_EVENT_FILE")
            .ok_or("input-probe requires explicit FLUXWM_FIXTURE_EVENT_FILE new regular path")?;
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_CLOEXEC | libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|e| format!("event file: {e}"))?;
        if !file.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("event file is not regular".into());
        }
        state.event_file = Some(file);
        evidence["event_file"] = json!(path);
        let socket = display_path()?;
        evidence["socket"] = json!(socket);
        let connection = connect(&socket, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        connection.display().get_registry(&qh, ());
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "input probe registry",
            |s| s.last_sync >= sync,
        )?;
        let sync = state.sync(&connection, &qh);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "input probe bindings",
            |s| s.last_sync >= sync,
        )?;
        if state.compositor.is_none()
            || state.shm.is_none()
            || state.wm.is_none()
            || state.pointer.is_none()
        {
            return Err("input probe globals/pointer missing".into());
        }
        for title in ["input-A", "input-B"] {
            let index = state.create(&qh, title);
            state.windows[index].toplevel.set_title(title.into());
            state.windows[index]
                .toplevel
                .set_app_id("org.fluxwm.InputProbe".into());
            state.windows[index].toplevel.set_min_size(320, 240);
            state.windows[index].surface.commit();
        }
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "input probe both configures",
            |s| s.windows.len() == 2 && s.windows.iter().all(|w| w.configures >= 1),
        )?;
        for index in 0..2 {
            let mut file = memory_file()?;
            let color = if index == 0 {
                0xffff0000_u32
            } else {
                0xff00ff00_u32
            };
            let bytes = color.to_ne_bytes().repeat(320 * 240);
            file.set_len(bytes.len() as u64)
                .map_err(|e| e.to_string())?;
            file.write_all(&bytes).map_err(|e| e.to_string())?;
            let pool =
                state
                    .shm
                    .as_ref()
                    .unwrap()
                    .create_pool(file.as_fd(), bytes.len() as i32, &qh, ());
            let buffer =
                pool.create_buffer(0, 320, 240, 320 * 4, wl_shm::Format::Xrgb8888, &qh, ());
            pool.destroy();
            let w = &mut state.windows[index];
            w.xdg.set_window_geometry(0, 0, 320, 240);
            w.surface.attach(Some(&buffer), 0, 0);
            w.surface.damage(0, 0, 320, 240);
            w.surface.frame(&qh, Callback::Frame(index));
            w.surface.commit();
            w.buffer = Some(buffer);
            w._storage = Some(file);
        }
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "input probe both frames",
            |s| s.windows.iter().all(|w| w.frames >= 1),
        )?;
        state.record(json!({"type":"ready","root":null,"roots":[{"root":0,"title":"input-A"},{"root":1,"title":"input-B"}],"observation_ms":OBSERVE_MS}));
        evidence["ready_at_ms"] = json!(start.elapsed().as_millis());
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(OBSERVE_MS),
            deadline,
        )?;
        state.record(json!({"type":"complete","root":null,"mapped_through_observation":true}));
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        evidence["mapped_through_observation"] = json!(true);
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["event_file_bytes"] = json!(state.event_file_bytes);
    evidence["events"] = json!(state.events);
    evidence
}
fn protocol_roundtrip(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
) -> Result<()> {
    let sync = state.sync(connection, &queue.handle());
    until(
        connection,
        queue,
        state,
        deadline,
        "protocol roundtrip",
        |s| s.last_sync >= sync,
    )
}
fn health_registry(
    connection: &Connection,
    queue: &mut EventQueue<State>,
    state: &mut State,
    deadline: Instant,
) -> Result<wl_registry::WlRegistry> {
    let registry = connection.display().get_registry(&queue.handle(), ());
    protocol_roundtrip(connection, queue, state, deadline)?;
    protocol_roundtrip(connection, queue, state, deadline)?;
    if state.wm.is_none() {
        return Err("missing XDG shell".into());
    }
    Ok(registry)
}
fn health_destroy_windows(state: &mut State) {
    for window in state.windows.iter_mut().filter(|w| !w.destroyed) {
        window.toplevel.destroy();
        window.xdg.destroy();
        window.surface.destroy();
        if let Some(buffer) = window.buffer.take() {
            buffer.destroy();
        }
        window.destroyed = true;
    }
}
fn health_no_redraw(before: &Json, after: &Json) -> Result<()> {
    for name in ["redraws", "compositions", "submissions", "frames_sent"] {
        if before["runtime"][name].as_u64().is_none()
            || before["runtime"][name] != after["runtime"][name]
        {
            return Err(format!(
                "health changed render counter {name}: {} -> {}",
                before["runtime"][name], after["runtime"][name]
            ));
        }
    }
    Ok(())
}
fn configure_window(diagnostics: &mut Diagnostics, title: &str, deadline: Instant) -> Result<Json> {
    diagnostics.call("windows.list", json!({}), deadline)?["windows"]
        .as_array()
        .ok_or("window list missing")?
        .iter()
        .find(|w| w["title"] == title || w["id"] == title)
        .cloned()
        .ok_or_else(|| format!("window {title} missing"))
}

/// Deliberately read configures without acknowledging them. A separate healthy
/// Wayland connection continues frame/roundtrip work during the resize burst.
fn configure_stream(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut evidence = json!({"ok":false,"mode":"configure-stream"});
    let mut state = State::new(start);
    let mut healthy = State::new(start);
    let result = (|| -> Result<()> {
        let path = display_path()?;
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let root = state.create(&qh, "configure-slow");
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "slow initial configure",
            |s| s.windows[root].configures > 0,
        )?;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "slow initial frame",
            |s| s.windows[root].frames > 0,
        )?;

        let sibling = connect(&path, deadline)?;
        let mut sibling_queue = sibling.new_event_queue::<State>();
        let sibling_qh = sibling_queue.handle();
        health_registry(&sibling, &mut sibling_queue, &mut healthy, deadline)?;
        let other = healthy.create(&sibling_qh, "configure-healthy");
        healthy.windows[other].surface.commit();
        until(
            &sibling,
            &mut sibling_queue,
            &mut healthy,
            deadline,
            "healthy initial configure",
            |s| s.windows[other].configures > 0,
        )?;
        healthy.map(other, &sibling_qh)?;
        until(
            &sibling,
            &mut sibling_queue,
            &mut healthy,
            deadline,
            "healthy initial frame",
            |s| s.windows[other].frames > 0,
        )?;

        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit FLUXWM_SOCKET required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        diagnostics.call(
            "action",
            json!({"name":"focus","direction":"left"}),
            deadline,
        )?;
        // Ack and commit setup state so subsequent ack-without-commit checks
        // compare against a known earlier committed serial.
        for _ in 0..3 {
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            protocol_roundtrip(&sibling, &mut sibling_queue, &mut healthy, deadline)?;
            state.windows[root].surface.commit();
            healthy.windows[other].surface.commit();
        }
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let before = configure_window(&mut diagnostics, "FluxWM fixture configure-slow", deadline)?;
        if before["focused"] != true {
            return Err("slow root not selected for resize".into());
        }
        let old_id = before["id"].clone();
        let identity = old_id.as_str().ok_or("missing window ID")?;
        let old_committed = before["configure"]["committed_serial"].clone();
        evidence["before"] = before;
        let received_before = state.windows[root].configure_serials.len();
        let healthy_frames = healthy.windows[other].frames;
        state.windows[root].ack_configures = false;
        diagnostics.call("action", json!({"name":"resize","delta":32.0}), deadline)?;
        for i in 0..128 {
            let delta = if i % 2 == 0 { 16.0 } else { -16.0 };
            diagnostics.call("action", json!({"name":"resize","delta":delta}), deadline)?;
            if i % 16 == 15 {
                protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
                let frames = healthy.windows[other].frames;
                healthy.windows[other].surface.damage(0, 0, 1, 1);
                healthy.windows[other]
                    .surface
                    .frame(&sibling_qh, Callback::Frame(other));
                healthy.windows[other].surface.commit();
                until(
                    &sibling,
                    &mut sibling_queue,
                    &mut healthy,
                    deadline,
                    "healthy frame during stalled configure",
                    |s| s.windows[other].frames > frames,
                )?;
            }
        }
        diagnostics.call("action", json!({"name":"resize","delta":64.0}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let stalled = configure_window(&mut diagnostics, identity, deadline)?;
        let received_after = state.windows[root].configure_serials.len();
        evidence["resize_actions"] = json!(130);
        evidence["wire_configures_during_stall"] = json!(received_after - received_before);
        evidence["healthy_frames_during_stall"] =
            json!(healthy.windows[other].frames - healthy_frames);
        evidence["stalled"] = stalled.clone();
        if received_after - received_before != 1 {
            return Err("unacknowledged resize stream emitted more than one wire configure".into());
        }
        if stalled["configure"]["unacknowledged"] != 1 || stalled["configure"]["deferred"] != true {
            return Err(
                "stalled stream must retain one wire state and a deferred latest state".into(),
            );
        }
        let serial = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].xdg.ack_configure(serial);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let flushed = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["after_old_ack_without_commit"] = flushed.clone();
        if state.windows[root].configure_serials.len() != received_after + 1
            || json!(state.windows[root].configure_sizes.last())
                != stalled["configure"]["desired_size"]
            || flushed["configure"]["unacknowledged"] != 1
            || flushed["configure"]["deferred"] != false
            || flushed["configure"]["last_acked_serial"] != serial
            || flushed["configure"]["committed_serial"] != old_committed
        {
            return Err(
                "ack did not flush exactly the latest state while preserving committed ownership"
                    .into(),
            );
        }
        let latest = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].xdg.ack_configure(latest);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let acked = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["after_latest_ack_without_commit"] = acked.clone();
        if acked["configure"]["unacknowledged"] != 0
            || acked["configure"]["last_acked_serial"] != latest
            || acked["configure"]["committed_serial"] != old_committed
        {
            return Err("ack was incorrectly treated as a surface commit".into());
        }
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let committed = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["after_commit"] = committed.clone();
        if committed["configure"]["committed_serial"] != latest
            || committed["configure"]["committed_size"] != stalled["configure"]["desired_size"]
        {
            return Err("buffer-preserving commit failed to apply the accepted configure".into());
        }

        // Pending changes can return to the already-sent state. Freeing the
        // slot must not echo an identical configure and create an ack loop.
        let count = state.windows[root].configure_serials.len();
        for delta in [16.0, 16.0, -16.0] {
            diagnostics.call("action", json!({"name":"resize","delta":delta}), deadline)?;
        }
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let reverted = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["returned_to_sent_state"] = reverted.clone();
        if reverted["configure"]["unacknowledged"] != 1
            || reverted["configure"]["deferred"] != false
        {
            return Err(
                "returning to the sent state retained a redundant deferred configure".into(),
            );
        }
        let serial = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].xdg.ack_configure(serial);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.windows[root].configure_serials.len() != count + 1 {
            return Err("unchanged pending state echoed another configure on ack".into());
        }
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;

        // Old ack after null unmap must not create a fresh initial configure.
        diagnostics.call("action", json!({"name":"resize","delta":16.0}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let in_flight = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let count = state.windows[root].configure_serials.len();
        state.windows[root].xdg.ack_configure(in_flight);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["old_ack_after_unmap"] = unmapped.clone();
        if state.windows[root].configure_serials.len() != count
            || unmapped["configure"]["initial_sent"] != false
            || unmapped["mapped"] != false
        {
            return Err("old ack sent an early remap configure".into());
        }
        state.windows[root].ack_configures = true;
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "fresh remap configure",
            |s| s.windows[root].configure_serials.len() > count,
        )?;
        let frames = state.windows[root].frames;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "first remap frame",
            |s| s.windows[root].frames > frames,
        )?;
        for _ in 0..3 {
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }

        // A remap handshake may coexist with one older in-flight configure.
        // Acking the new initial serial consumes both through Smithay's rules.
        state.windows[root].ack_configures = false;
        diagnostics.call("action", json!({"name":"resize","delta":16.0}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let count = state.windows[root].configure_serials.len();
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "remap with old configure in flight",
            |s| s.windows[root].configure_serials.len() > count,
        )?;
        let overlap = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["remap_overlap"] = overlap.clone();
        if overlap["configure"]["unacknowledged"] != 2 || overlap["mapped"] != false {
            return Err(
                "remap must admit one initial configure alongside the old in-flight configure"
                    .into(),
            );
        }
        let initial = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].xdg.ack_configure(initial);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let consumed = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["remap_latest_ack"] = consumed.clone();
        if consumed["configure"]["unacknowledged"] != 0 {
            return Err("new initial ack did not retire the older serial".into());
        }
        state.windows[root].ack_configures = true;
        let frames = state.windows[root].frames;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "overlap remap frame",
            |s| s.windows[root].frames > frames,
        )?;
        for _ in 0..3 {
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }

        state.windows[root].ack_configures = false;
        diagnostics.call("action", json!({"name":"resize","delta":16.0}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let retiring = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["before_destroy"] = retiring.clone();
        if retiring["configure"]["unacknowledged"] != 1 {
            return Err("destruction case lacks outstanding configure".into());
        }
        health_destroy_windows(&mut state);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let after_destroy = diagnostics.call("windows.list", json!({}), deadline)?;
        evidence["after_destroy"] = after_destroy.clone();
        if after_destroy["windows"]
            .as_array()
            .ok_or("missing windows")?
            .len()
            != 1
        {
            return Err("destroyed configure owner remained live".into());
        }
        let replacement = state.create(&qh, "configure-replacement");
        state.windows[replacement].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "replacement initial configure",
            |s| s.windows[replacement].configures > 0,
        )?;
        state.map(replacement, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "replacement frame",
            |s| s.windows[replacement].frames > 0,
        )?;
        let new = configure_window(
            &mut diagnostics,
            "FluxWM fixture configure-replacement",
            deadline,
        )?;
        evidence["replacement"] = new.clone();
        if new["id"] == old_id || new["mapped"] != true {
            return Err("replacement reused a stale window identity".into());
        }
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence["healthy_events"] = json!(healthy.events);
    evidence
}

/// Admit six windows as the parent reloads fixed, fraction and preset defaults.
/// Existing windows stay live to expose accidental retroactive width changes.
fn config_width(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"config-width"});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        for (index, expected) in [
            json!({"logical_px":400.0}),
            json!({"logical_px":760.0}),
            json!({"fraction":0.25}),
            json!({"logical_px":600.0}),
            json!({"logical_px":600.0}),
            json!({"logical_px":480.0}),
        ]
        .into_iter()
        .enumerate()
        {
            loop {
                let effective = diagnostics.call("config.effective", json!({}), deadline)?;
                let status = diagnostics.call("diagnostics", json!({}), deadline)?;
                let failed = status["configuration"]["status"] == "failed";
                if effective["effective"]["layout"]["default_width"] == expected
                    && (index != 4 || failed)
                {
                    break;
                }
                observe_for(
                    &connection,
                    &mut queue,
                    &mut state,
                    Duration::from_millis(10),
                    deadline,
                )?;
            }
            let root = state.create(&qh, &format!("width-{index}"));
            state.windows[root].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "width initial configure",
                |s| s.windows[root].configures > 0,
            )?;
            state.map(root, &qh)?;
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "width initial frame",
                |s| s.windows[root].frames > 0,
            )?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            state.windows[root]
                .toplevel
                .set_title(format!("width-{index}-ready"));
            state.windows[root].surface.commit();
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(1500),
            deadline,
        )?;
        evidence["windows"]=json!(state.windows.iter().enumerate().map(|(index,w)|json!({"index":index,"frames":w.frames,"configure_sizes":w.configure_sizes})).collect::<Vec<_>>());
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["events"] = json!(state.events);
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}

/// Keep a wide mapped member alive through rejected and recovered logical reloads.
fn config_geometry(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"config-geometry","minimum_width":999_999_984});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let root = state.create(&qh, "config-wide");
        state.windows[root].toplevel.set_min_size(999_999_984, 240);
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "wide initial configure",
            |s| s.windows[root].configures > 0,
        )?;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "wide frame",
            |s| s.windows[root].frames > 0,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[root]
            .toplevel
            .set_title("config-wide-ready".into());
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        evidence["ready_frames"] = json!(state.windows[root].frames);
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(6500),
            deadline,
        )?;
        if let Some(error) = state.error.take() {
            return Err(error);
        }
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["events"] = json!(state.events);
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}

/// Layout rejection must not confuse buffer ownership with scene membership.
/// Keep the client alive so it can unmap and retry smaller hints.
fn configure_admission(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"configure-admission"});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let healthy = state.create(&qh, "admission-healthy");
        state.windows[healthy].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "admission healthy configure",
            |s| s.windows[healthy].configures > 0,
        )?;
        state.map(healthy, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "admission healthy frame",
            |s| s.windows[healthy].frames > 0,
        )?;
        let root = state.create(&qh, "admission-retry");
        state.windows[root]
            .toplevel
            .set_min_size(1_000_000_000, 240);
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "rejected root initial configure",
            |s| s.windows[root].configures > 0,
        )?;
        state.map(root, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit FLUXWM_SOCKET required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let rejected =
            configure_window(&mut diagnostics, "FluxWM fixture admission-retry", deadline)?;
        let rejected_sequence = diagnostics.last_sequence;
        evidence["rejected"] = rejected.clone();
        let identity = rejected["id"]
            .as_str()
            .ok_or("rejected role has no identity")?;
        let count = state.windows[root].configure_serials.len();
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["after_null_unmap"] = unmapped.clone();
        evidence["wire_configures_on_null_unmap"] =
            json!(state.windows[root].configure_serials.len() - count);
        // This real wire oracle runs before checking newly added diagnostics,
        // so the preserved binary fails for behavior rather than absent fields.
        if state.windows[root].configure_serials.len() != count {
            return Err(
                "rejected-admission null unmap incorrectly emitted an initial configure".into(),
            );
        }
        if diagnostics.last_sequence <= rejected_sequence {
            return Err("rejected-role buffer retirement did not advance IPC sequence".into());
        }
        if rejected["buffered"] != true
            || rejected["mapped"] != false
            || rejected["admission_error"].as_str().is_none()
            || unmapped["buffered"] != false
            || unmapped["mapped"] != false
            || !unmapped["admission_error"].is_null()
            || unmapped["configure"]["initial_sent"] != false
        {
            return Err(
                "admission diagnostics conflate root buffer presence and layout membership".into(),
            );
        }
        state.windows[root]
            .toplevel
            .set_title("FluxWM fixture admission-recovered".into());
        state.windows[root].toplevel.set_min_size(320, 240);
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "admission retry fresh initial configure",
            |s| s.windows[root].configure_serials.len() > count,
        )?;
        let frames = state.windows[root].frames;
        state.map(root, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "admission retry maps after smaller hints",
            |s| s.windows[root].frames > frames,
        )?;
        let recovered = configure_window(&mut diagnostics, identity, deadline)?;
        evidence["recovered"] = recovered.clone();
        if recovered["mapped"] != true
            || recovered["buffered"] != true
            || !recovered["admission_error"].is_null()
        {
            return Err("smaller valid hints did not recover the retained role".into());
        }
        let frames = state.windows[healthy].frames;
        state.windows[healthy].surface.damage(0, 0, 1, 1);
        state.windows[healthy]
            .surface
            .frame(&qh, Callback::Frame(healthy));
        state.windows[healthy].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "healthy same-client role survives admission rejection",
            |s| s.windows[healthy].frames > frames,
        )?;
        evidence["healthy_frame_after_recovery"] = json!(true);

        // Aggregate capacity can recover without any new commit from a static
        // rejected client. Its originally requested frame must then complete.
        let blocker = state.create(&qh, "admission-blocker");
        state.windows[blocker]
            .toplevel
            .set_min_size(600_000_000, 240);
        state.windows[blocker].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "aggregate blocker configure",
            |s| s.windows[blocker].configures > 0,
        )?;
        state.map(blocker, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "aggregate blocker frame",
            |s| s.windows[blocker].frames > 0,
        )?;
        let waiting = state.create(&qh, "admission-static");
        state.windows[waiting]
            .toplevel
            .set_min_size(500_000_000, 240);
        state.windows[waiting].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "aggregate waiting configure",
            |s| s.windows[waiting].configures > 0,
        )?;
        state.map(waiting, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let rejected = configure_window(
            &mut diagnostics,
            "FluxWM fixture admission-static",
            deadline,
        )?;
        evidence["aggregate_rejected"] = rejected.clone();
        if rejected["buffered"] != true
            || rejected["mapped"] != false
            || rejected["admission_error"].as_str().is_none()
        {
            return Err("aggregate case lacks buffered rejected role".into());
        }
        state.windows[blocker].toplevel.destroy();
        state.windows[blocker].xdg.destroy();
        state.windows[blocker].surface.destroy();
        state.windows[blocker].destroyed = true;
        if let Some(buffer) = state.windows[blocker].buffer.take() {
            buffer.destroy();
        }
        // No wl_surface.commit or replacement-buffer request on waiting from
        // here onward. Normal configure acknowledgments remain enabled.
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let automatic = configure_window(
            &mut diagnostics,
            "FluxWM fixture admission-static",
            deadline,
        )?;
        evidence["aggregate_after_blocker_destroy"] = automatic.clone();
        if automatic["mapped"] != true
            || automatic["buffered"] != true
            || !automatic["admission_error"].is_null()
            || automatic["id"] != rejected["id"]
            || automatic["focused"] != false
        {
            return Err("static rejected role did not recover without another surface commit, or stole focus".into());
        }
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "static retained buffer gets frame after capacity returns",
            |s| s.windows[waiting].frames > 0,
        )?;
        evidence["aggregate_recovery_without_new_surface_commit"] = json!(true);
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["events"] = json!(state.events);
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}

fn shell_health(timeout: Duration, mode: &str) -> Json {
    let capacity = mode == "shell-health-capacity";
    let disconnect = mode == "shell-health-disconnect";
    let start = Instant::now();
    let deadline = start + timeout;
    let mut evidence = json!({"ok":false,"mode":mode});
    let mut state = State::new(start);
    state.health_mode = !capacity;
    state.health_ignore = !capacity;
    let result = (|| -> Result<()> {
        let path = display_path()?;
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        let registry = health_registry(&connection, &mut queue, &mut state, deadline)?;
        state.health_primary = state.wm.clone();
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit FLUXWM_SOCKET required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let snapshot = diagnostics.call("diagnostics", json!({}), deadline)?;
        if snapshot.get("shell_health").is_none() {
            return Err("compositor has no shell health diagnostics".into());
        }
        if snapshot["shell_health"]["timeout_ms"] != json!(2000)
            || snapshot["shell_health"]["interval_ms"] != json!(5000)
        {
            return Err("configured shell health policy differs from 2000/5000 ms".into());
        }
        if disconnect {
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "disconnect real ping",
                |s| s.health_serials.len() == 1,
            )?;
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(2200),
                deadline,
            )?;
            let stalled = diagnostics.call("diagnostics", json!({}), deadline)?;
            let clients = stalled["shell_health"]["clients"]
                .as_array()
                .ok_or("missing stalled clients")?;
            if clients.len() != 1
                || clients[0]["status"] != "unresponsive"
                || stalled["shell_health"]["timer_armed"] != false
            {
                return Err(format!(
                    "disconnect must begin with live timed-out binding and no timer: {stalled}"
                ));
            }
            evidence["before_disconnect"] = stalled;
            evidence["disconnect_with_live_binding"] = json!(true);
            evidence["explicit_shell_destroy_sent"] = json!(false);
            return Ok(()); // Process exit closes the live connection; no destroy request.
        }
        let global = state
            .globals
            .iter()
            .find(|g| g["interface"] == "xdg_wm_base")
            .ok_or("XDG registry name missing")?;
        let name = global["name"].as_u64().ok_or("invalid registry name")? as u32;
        let version = global["version"]
            .as_u64()
            .ok_or("invalid shell version")?
            .min(7) as u32;
        if capacity {
            let original = state.wm.as_ref().unwrap().clone();
            let mut extra = Vec::<xdg_wm_base::XdgWmBase>::new();
            for _ in 0..1025 {
                extra.push(registry.bind(name, version, &qh, ()));
            }
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            // Dispatch/flush the ping burst, then order the diagnostics after all
            // pongs with an explicit display roundtrip.
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let capped = diagnostics.call("diagnostics", json!({}), deadline)?;
            let health = &capped["shell_health"];
            if health["capacity"] != json!(1024)
                || health["clients"].as_array().map(Vec::len) != Some(1024)
                || health["stats"]["capacity_rejections"] != json!(2)
                || health["stats"]["sent"] != json!(1024)
                || health["stats"]["pongs"] != json!(1024)
            {
                return Err(format!("monitor capacity oracle failed: {health}"));
            }
            evidence["capacity_snapshot"] = capped;
            state.wm = Some(extra.last().unwrap().clone());
            let window = state.create(&qh, "health-unmonitored-overflow");
            state.windows[window].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "overflow configure",
                |s| s.windows[window].configures > 0,
            )?;
            state.map(window, &qh)?;
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "overflow frame",
                |s| s.windows[window].frames > 0,
            )?;
            let windows = diagnostics.call("windows.list", json!({}), deadline)?;
            if windows["windows"][0]["responsiveness"]["status"] != "unmonitored" {
                return Err(format!("overflow role claimed monitored: {windows}"));
            }
            evidence["overflow_window"] = windows;
            health_destroy_windows(&mut state);
            for wm in extra {
                wm.destroy();
            }
            original.destroy();
            state.wm = None;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        } else {
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "initial real ping",
                |s| !s.health_serials.is_empty(),
            )?;
            let serial = state.health_serials[0];
            let wm = state.wm.as_ref().unwrap().clone();
            // A second binding on this connection responds normally. Monitor
            // ownership is xdg_wm_base resource, not just connection identity.
            let extra: xdg_wm_base::XdgWmBase = registry.bind(name, version, &qh, ());
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            wm.pong(serial.wrapping_add(1));
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let before = diagnostics.call("diagnostics", json!({}), deadline)?;
            let states = before["shell_health"]["clients"]
                .as_array()
                .ok_or("missing health clients")?;
            if states.len() != 2
                || !states.iter().any(|e| e["status"] == "waiting")
                || !states.iter().any(|e| e["status"] == "responsive")
                || before["shell_health"]["stats"]["pongs"] != json!(1)
            {
                return Err(format!(
                    "wrong pong or shared binding ownership accepted: {before}"
                ));
            }
            evidence["before_timeout"] = before.clone();
            let before_sequence = diagnostics.last_sequence.unwrap();
            evidence["before_timeout_sequence"] = json!(before_sequence);
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(1800),
                deadline,
            )?;
            let early = diagnostics.call("diagnostics", json!({}), deadline)?;
            if early["shell_health"]["stats"]["timeouts"] != json!(0)
                || !early["shell_health"]["clients"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|c| c["status"] == "waiting")
            {
                return Err(format!("probe expired before 1.8-second check: {early}"));
            }
            evidence["before_deadline"] = early;
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(400),
                deadline,
            )?;
            let expired = diagnostics.call("diagnostics", json!({}), deadline)?;
            if expired["shell_health"]["stats"]["timeouts"] != json!(1)
                || expired["shell_health"]["stats"]["timer_fired"]
                    .as_u64()
                    .unwrap_or(0)
                    < 1
                || !expired["shell_health"]["clients"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["status"] == "unresponsive")
                || expired["runtime"]["wayland_requests"] != before["runtime"]["wayland_requests"]
            {
                return Err(format!("silent timer expiry oracle failed: {expired}"));
            }
            if diagnostics.last_sequence.unwrap() <= before_sequence {
                return Err("timeout did not advance IPC sequence".into());
            }
            health_no_redraw(&before, &expired)?;
            evidence["no_redraw_during_timeout"] = json!(true);
            evidence["expired_sequence"] = json!(diagnostics.last_sequence.unwrap());
            evidence["expired"] = expired;
            // A different binding remains usable while the first is timed out.
            state.wm = Some(extra.clone());
            let window = state.create(&qh, "health-responsive-sibling");
            state.windows[window].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "healthy sibling configure",
                |s| s.windows[window].configures > 0,
            )?;
            state.map(window, &qh)?;
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "healthy sibling frame",
                |s| s.windows[window].frames > 0,
            )?;
            state.wm = Some(wm.clone());
            evidence["sibling_frame_while_primary_unresponsive"] =
                json!(state.windows[window].frames);
            health_destroy_windows(&mut state);
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(150),
                deadline,
            )?;
            let before_recovery = diagnostics.call("diagnostics", json!({}), deadline)?;
            if before_recovery["mapped_windows"] != json!(0) {
                return Err("recovery render comparison requires no mapped windows".into());
            }
            evidence["before_recovery"] = before_recovery.clone();
            wm.pong(serial);
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let recovered = diagnostics.call("diagnostics", json!({}), deadline)?;
            if recovered["shell_health"]["stats"]["recoveries"] != json!(1)
                || recovered["shell_health"]["stats"]["pongs"] != json!(2)
                || recovered["shell_health"]["clients"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|e| e["status"] != "responsive")
            {
                return Err(format!("late matching pong failed: {recovered}"));
            }
            health_no_redraw(&before_recovery, &recovered)?;
            evidence["no_redraw_during_recovery"] = json!(true);
            evidence["recovered"] = recovered;
            // A replay is ignored; the next periodic ping needs its new serial.
            wm.pong(serial);
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let replay = diagnostics.call("diagnostics", json!({}), deadline)?;
            if replay["shell_health"]["stats"]["pongs"] != json!(2) {
                return Err("replayed pong accepted".into());
            }
            state.health_ignore = false;
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(4700),
                deadline,
            )?;
            if state.health_serials.len() != 1 {
                return Err("primary was re-probed before 4.7-second check".into());
            }
            evidence["no_reprobe_at_ms"] = json!(4700);
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(500),
                deadline,
            )?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let periodic = diagnostics.call("diagnostics", json!({}), deadline)?;
            if state.health_serials.len() != 2
                || state.health_serials[0] == state.health_serials[1]
                || periodic["shell_health"]["stats"]["pongs"]
                    .as_u64()
                    .unwrap_or(0)
                    < 4
                || periodic["shell_health"]["stats"]["timeouts"] != json!(1)
            {
                return Err(format!("periodic re-probe failed: {periodic}"));
            }
            evidence["periodic"] = periodic;
            health_destroy_windows(&mut state);
            extra.destroy();
            wm.destroy();
            state.wm = None;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }
        let retired = diagnostics.call("diagnostics", json!({}), deadline)?;
        if retired["shell_health"]["clients"] != json!([])
            || retired["shell_health"]["timer_armed"] != false
        {
            return Err(format!("health resources/timer not retired: {retired}"));
        }
        evidence["retired"] = retired;
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["events"] = json!(state.events);
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if let [mode, path] = args.as_slice()
        && mode == "verify-container-clip"
    {
        let result = verify_container_ppm(Path::new(path));
        println!("{result}");
        if result["ok"] != json!(true) {
            std::process::exit(1);
        }
        return;
    }
    let parsed = match args.as_slice() {
        [mode]
            if mode == "shell-health"
                || mode == "shell-health-capacity"
                || mode == "shell-health-disconnect" =>
        {
            Ok((mode.as_str(), Duration::from_millis(12000)))
        }
        [mode, flag, value]
            if (mode == "shell-health"
                || mode == "shell-health-capacity"
                || mode == "shell-health-disconnect")
                && flag == "--timeout-ms" =>
        {
            value
                .parse::<u64>()
                .ok()
                .filter(|n| (1..=15000).contains(n))
                .map(|n| (mode.as_str(), Duration::from_millis(n)))
                .ok_or("shell-health timeout must be 1..15000ms")
        }
        [mode] if mode == "input-probe" || mode == "config-geometry" || mode == "config-width" => {
            Ok((mode.as_str(), Duration::from_millis(10000)))
        }
        [mode, flag, value]
            if (mode == "input-probe" || mode == "config-geometry" || mode == "config-width")
                && flag == "--timeout-ms" =>
        {
            value
                .parse::<u64>()
                .ok()
                .filter(|n| (1..=10000).contains(n))
                .map(|n| (mode.as_str(), Duration::from_millis(n)))
                .ok_or("input-probe timeout must be1..10000ms")
        }
        [mode]
            if mode == "lifecycle"
                || mode == "slow-reader"
                || mode == "pointer-feedback"
                || mode == "popup-lifecycle"
                || mode == "container-clip"
                || mode == "popup-clip"
                || mode == "configure-stream"
                || mode == "configure-admission" =>
        {
            Ok((mode.as_str(), Duration::from_secs(5)))
        }
        [mode, flag, value]
            if (mode == "lifecycle"
                || mode == "slow-reader"
                || mode == "pointer-feedback"
                || mode == "popup-lifecycle"
                || mode == "container-clip"
                || mode == "popup-clip"
                || mode == "configure-stream"
                || mode == "configure-admission")
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
            "usage: fluxwm-fixture lifecycle|config-width|config-geometry|configure-stream|configure-admission|slow-reader|pointer-feedback|popup-lifecycle|container-clip|popup-clip|input-probe|shell-health|shell-health-capacity|shell-health-disconnect [--timeout-ms N; maximum depends on mode]",
        ),
    };
    let result = match parsed {
        Ok(("configure-stream", timeout)) => configure_stream(timeout),
        Ok(("config-width", timeout)) => config_width(timeout),
        Ok(("config-geometry", timeout)) => config_geometry(timeout),
        Ok(("configure-admission", timeout)) => configure_admission(timeout),
        Ok(("slow-reader", timeout)) => slow_reader(timeout),
        Ok(("pointer-feedback", timeout)) => pointer_feedback(timeout),
        Ok(("popup-lifecycle", timeout)) => popup_lifecycle(timeout),
        Ok(("container-clip", timeout)) => container_clip(timeout),
        Ok(("popup-clip", timeout)) => popup_clip(timeout),
        Ok(("input-probe", timeout)) => input_probe(timeout),
        Ok((
            mode @ ("shell-health" | "shell-health-capacity" | "shell-health-disconnect"),
            timeout,
        )) => shell_health(timeout, mode),
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
            last_sequence: None,
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
            last_sequence: None,
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
    fn oversized_pattern_preserves_original_quadrants_and_distinct_cyan_overflow() {
        let oversized = container_pixels();
        let original = pixels(0);
        assert_eq!(oversized.len(), 1900 * 240 * 4);
        for y in 0..240 {
            assert_eq!(
                &oversized[y * 1900 * 4..y * 1900 * 4 + 320 * 4],
                &original[y * 320 * 4..(y + 1) * 320 * 4]
            );
            assert!(
                oversized[y * 1900 * 4 + 320 * 4..(y + 1) * 1900 * 4]
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .all(|p| *p == 0xff00ffff_u32.to_ne_bytes())
            );
        }
    }
    #[test]
    fn container_checker_accepts_exact_clip_and_rejects_one_pixel_leak() {
        let original = pixels(0);
        let mut ppm = b"P6\n320 240\n255\n".to_vec();
        for y in 0..240 {
            for x in 0..320 {
                let value = if x < 280 {
                    let at = (y * 320 + x) * 4;
                    let pixel = u32::from_ne_bytes(original[at..at + 4].try_into().unwrap());
                    [(pixel >> 16) as u8, (pixel >> 8) as u8, pixel as u8]
                } else {
                    [15, 18, 23]
                };
                ppm.extend(value);
            }
        }
        let path = std::env::temp_dir().join(format!("fluxwm-clip-checker-{}", std::process::id()));
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .unwrap();
        file.write_all(&ppm).unwrap();
        drop(file);
        assert_eq!(verify_container_ppm(&path)["ok"], json!(true));
        let header = b"P6\n320 240\n255\n".len();
        ppm[header + 280 * 3..header + 281 * 3].copy_from_slice(&[0, 255, 255]);
        std::fs::write(&path, &ppm).unwrap();
        let bad = verify_container_ppm(&path);
        std::fs::remove_file(path).unwrap();
        assert_eq!(bad["ok"], json!(false));
        assert_eq!(bad["rgb_counts"]["cyan"], json!(1));
    }
    #[test]
    fn input_event_file_is_live_private_and_matches_retained_records() {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let path =
            std::env::temp_dir().join(format!("fluxwm-input-event-test-{}", std::process::id()));
        let file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&path)
            .unwrap();
        let mut state = State::new(Instant::now());
        state.event_file = Some(file);
        state.record(json!({"type":"pointer.enter","root":0,"x":1.0,"y":2.0}));
        let first = std::fs::read_to_string(&path).unwrap();
        assert!(first.ends_with('\n'));
        assert_eq!(
            serde_json::from_str::<Json>(first.trim()).unwrap(),
            state.events[0]
        );
        state.record(json!({"type":"pointer.button","root":0,"button":272,"state":"pressed","serial":1,"time":1}));
        let all = std::fs::read_to_string(&path).unwrap();
        let records: Vec<Json> = all
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records, state.events);
        assert_eq!(state.event_file_bytes, all.len());
        assert_eq!(
            std::fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        drop(state);
        std::fs::remove_file(path).unwrap();
    }
    #[test]
    fn missing_socket_fails_without_blocking() {
        let deadline = Instant::now() + Duration::from_millis(100);
        assert!(connect(Path::new("/no-such-fluxwm-fixture-socket"), deadline).is_err());
    }
}
