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
        wl_buffer, wl_callback, wl_compositor, wl_keyboard, wl_pointer, wl_region, wl_registry,
        wl_seat, wl_shm, wl_shm_pool, wl_subcompositor, wl_subsurface, wl_surface,
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
    capability_values: Vec<u32>,
    activated: bool,
    maximized: bool,
    fullscreen: bool,
    suspended: bool,
    destroyed: bool,
    allow_close: bool,
    closes: u32,
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
            capability_values: Vec::new(),
            activated: false,
            maximized: false,
            fullscreen: false,
            suspended: false,
            destroyed: false,
            allow_close: false,
            closes: 0,
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
    /// Allocate an exact-size client buffer; used for strict maximized geometry.
    fn map_sized(&mut self, index: usize, size: [i32; 2], qh: &QueueHandle<Self>) -> Result<()> {
        self.map_sized_pixels(index, size, false, qh)
    }
    fn map_sized_pixels(
        &mut self,
        index: usize,
        size: [i32; 2],
        transparent: bool,
        qh: &QueueHandle<Self>,
    ) -> Result<()> {
        let [width, height] = size;
        let length = i64::from(width) * i64::from(height) * 4;
        if width <= 0 || height <= 0 || length > 32 * 1024 * 1024 {
            return Err(format!("fixture buffer dimensions out of bounds: {size:?}"));
        }
        let mut file = memory_file()?;
        file.set_len(length as u64).map_err(|e| e.to_string())?;
        let bytes = if transparent {
            (0..height)
                .flat_map(|_| {
                    (0..width).flat_map(|x| {
                        if x < width / 2 {
                            [0, 0, 255, 255]
                        } else {
                            [0, 0, 0, 0]
                        }
                    })
                })
                .collect::<Vec<_>>()
        } else {
            [0x40, 0x80, 0xc0, 0xff].repeat((length / 4) as usize)
        };
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        let pool = self
            .shm
            .as_ref()
            .unwrap()
            .create_pool(file.as_fd(), length as i32, qh, ());
        let buffer = pool.create_buffer(
            0,
            width,
            height,
            width * 4,
            if transparent {
                wl_shm::Format::Argb8888
            } else {
                wl_shm::Format::Xrgb8888
            },
            qh,
            (),
        );
        pool.destroy();
        let w = &mut self.windows[index];
        w.xdg.set_window_geometry(0, 0, width, height);
        w.surface.attach(Some(&buffer), 0, 0);
        w.surface.damage(0, 0, width, height);
        w.surface.frame(qh, Callback::Frame(index));
        w.surface.commit();
        if let Some(old) = w.buffer.replace(buffer) {
            old.destroy();
        }
        w._storage = Some(file);
        self.record(json!({"sized_map_commit":index,"size":size}));
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
            xdg_toplevel::Event::Configure {
                width,
                height,
                states,
            } => {
                state.windows[*index].pending_size = [width, height];
                state.windows[*index].activated = states.as_chunks::<4>().0.iter().any(|bytes| {
                    u32::from_ne_bytes(*bytes) == xdg_toplevel::State::Activated as u32
                });
                state.windows[*index].maximized = states.as_chunks::<4>().0.iter().any(|bytes| {
                    u32::from_ne_bytes(*bytes) == xdg_toplevel::State::Maximized as u32
                });
                state.windows[*index].fullscreen = states.as_chunks::<4>().0.iter().any(|bytes| {
                    u32::from_ne_bytes(*bytes) == xdg_toplevel::State::Fullscreen as u32
                });
                state.windows[*index].suspended = states.as_chunks::<4>().0.iter().any(|bytes| {
                    u32::from_ne_bytes(*bytes) == xdg_toplevel::State::Suspended as u32
                });
            }
            xdg_toplevel::Event::WmCapabilities { capabilities } => {
                state.windows[*index].capabilities += 1;
                state.record(json!({"wm_capabilities":index,"bytes":capabilities}));
                let mut values: Vec<_> = capabilities
                    .as_chunks::<4>()
                    .0
                    .iter()
                    .map(|bytes| u32::from_ne_bytes(*bytes))
                    .collect();
                // Capabilities are a protocol set; Smithay stores them in a
                // HashSet, so wire order is deliberately not an oracle.
                values.sort_unstable();
                state.windows[*index].capability_values = values.clone();
                if capabilities.len() % 4 != 0
                    || values
                        != [
                            xdg_toplevel::WmCapabilities::Maximize as u32,
                            xdg_toplevel::WmCapabilities::Fullscreen as u32,
                            xdg_toplevel::WmCapabilities::Minimize as u32,
                        ]
                {
                    state.error = Some(format!(
                        "window {index}: expected exactly Fullscreen, Maximize, Minimize capabilities, got {values:?}"
                    ));
                }
            }
            xdg_toplevel::Event::Close => {
                state.windows[*index].closes += 1;
                if !state.windows[*index].allow_close {
                    state.error = Some(format!("window {index}: unexpected close"));
                }
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
delegate_noop!(State: ignore wl_region::WlRegion);
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

/// Screenshot scene with an unfocused floating client above a focused tile.
fn floating_pixels(timeout: Duration, fullscreen: bool) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mode = if fullscreen {
        "floating-pixels-fullscreen"
    } else {
        "floating-pixels"
    };
    let mut state = State::new(start);
    let mut evidence = json!({"ok":false,"mode":mode});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut control = Diagnostics::new(Path::new(&ipc), deadline)?;
        let tile = state.create(&qh, "floating-pixel-tile");
        state.windows[tile].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pixel tile configure",
            |s| s.windows[tile].configures > 0,
        )?;
        state.map_sized(tile, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pixel tile frame",
            |s| s.windows[tile].frames > 0,
        )?;
        let floating = state.create(&qh, "floating-pixel-float");
        state.windows[floating].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pixel float configure",
            |s| s.windows[floating].configures > 0,
        )?;
        state.map(floating, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "pixel float frame",
            |s| s.windows[floating].frames > 0,
        )?;
        control.call("action", json!({"name":"floating","toggle":true}), deadline)?;
        control.call(
            "action",
            json!({"name":"focus","direction":"left"}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let focused =
            configure_window(&mut control, "FluxWM fixture floating-pixel-tile", deadline)?;
        let raised = configure_window(
            &mut control,
            "FluxWM fixture floating-pixel-float",
            deadline,
        )?;
        if focused["focused"] != true
            || focused["floating"] != false
            || raised["focused"] != false
            || raised["floating"] != true
        {
            return Err(format!(
                "pixel stacking focus precondition failed: {focused} / {raised}"
            ));
        }
        if fullscreen {
            state.windows[tile].toplevel.set_fullscreen(None);
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            if !state.windows[tile].fullscreen {
                return Err("focused tile failed fullscreen".into());
            }
        } else {
            // Ensure the visible tile actually crosses the float. A tile-sized
            // client buffer makes the stacking oracle independent of old small
            // client geometry and exercises occlusion by the focused tile.
            let g = &focused["geometry"];
            let size = [
                g["width"].as_f64().ok_or("tile width missing")? as i32,
                g["height"].as_f64().ok_or("tile height missing")? as i32,
            ];
            state.map_sized(tile, size, &qh)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let a = &raised["geometry"];
            if g["x"].as_f64().unwrap() >= a["x"].as_f64().unwrap() + f64::from(WIDTH)
                || g["x"].as_f64().unwrap() + g["width"].as_f64().unwrap()
                    <= a["x"].as_f64().unwrap()
                || g["y"].as_f64().unwrap() >= a["y"].as_f64().unwrap() + f64::from(HEIGHT)
                || g["y"].as_f64().unwrap() + g["height"].as_f64().unwrap()
                    <= a["y"].as_f64().unwrap()
            {
                return Err("focused tile does not overlap float in screenshot oracle".into());
            }
        }
        evidence["focused_tile"] =
            configure_window(&mut control, "FluxWM fixture floating-pixel-tile", deadline)?;
        evidence["unfocused_float"] = raised.clone();
        evidence["ready_at_ms"] = json!(start.elapsed().as_millis());
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(1000),
            deadline,
        )?;
        let settled = configure_window(
            &mut control,
            "FluxWM fixture floating-pixel-float",
            deadline,
        )?;
        if settled["geometry"] != raised["geometry"]
            || settled["floating"] != true
            || settled["focused"] != false
        {
            return Err("floating screenshot scene moved while captured".into());
        }
        evidence["after_settle"] = settled;
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence
}

/// Real floating transitions with client commits and same-client transient admission.
fn floating(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"floating"});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut control = Diagnostics::new(Path::new(&ipc), deadline)?;
        let root = state.create(&qh, "floating-root");
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "floating initial configure",
            |s| s.windows[root].configures > 0,
        )?;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "floating initial frame",
            |s| s.windows[root].frames > 0,
        )?;
        let tiled = configure_window(&mut control, "FluxWM fixture floating-root", deadline)?;
        if tiled["floating"] != false {
            return Err("ordinary initial role unexpectedly floated".into());
        }
        evidence["tiled"] = tiled.clone();
        control.call("action", json!({"name":"floating","toggle":true}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let floated = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if floated["floating"] != true
            || floated["geometry"]["width"] != json!(WIDTH as f64)
            || floated["geometry"]["height"] != json!(HEIGHT as f64)
            || state.windows[root].fullscreen
            || state.windows[root].maximized
        {
            return Err(format!(
                "float failed actual-size/no-overlay invariant: {floated}"
            ));
        }
        evidence["initial_float"] = floated.clone();
        control.call(
            "action",
            json!({"name":"move","direction":"right"}),
            deadline,
        )?;
        control.call("action", json!({"name":"resize","delta":64.0}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let moved = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if moved["geometry"]["x"].as_f64() != floated["geometry"]["x"].as_f64().map(|x| x + 32.0)
            || moved["geometry"]["width"].as_f64() != Some(f64::from(WIDTH) + 64.0)
        {
            return Err(format!("float keyboard move/resize failed: {moved}"));
        }
        // The target remains owned by the model even when an ordinary client
        // commits an older-sized buffer after acknowledging the resize.
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let committed = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if committed["geometry"] != moved["geometry"] {
            return Err("client commit overwrote floating target".into());
        }
        evidence["moved_resized"] = moved.clone();
        control.call(
            "action",
            json!({"name":"pan","dx":32.0,"dy":16.0}),
            deadline,
        )?;
        let panned = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if panned["geometry"] != moved["geometry"] {
            return Err("camera pan changed floating world rect".into());
        }
        evidence["pan_preserves_world_rect"] = json!(true);
        for mode in ["canvas", "strip"] {
            control.call(
                "action",
                json!({"name":"workspace-mode","mode":mode}),
                deadline,
            )?;
            let window = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
            if window["floating"] != true || window["geometry"] != moved["geometry"] {
                return Err(format!("{mode} mode lost floating geometry"));
            }
        }
        evidence["mode_restore"] = json!(true);
        for overlay in ["maximize", "fullscreen"] {
            control.call("action", json!({"name":overlay,"toggle":true}), deadline)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let over = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
            if over["floating"] != true
                || over[if overlay == "maximize" {
                    "maximized"
                } else {
                    overlay
                }] != true
            {
                return Err(format!("{overlay} lost floating intent"));
            }
            control.call("action", json!({"name":overlay,"toggle":false}), deadline)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let restored = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
            if restored["geometry"] != moved["geometry"] || restored["floating"] != true {
                return Err(format!("{overlay} restore lost floating rect"));
            }
            evidence[format!("{overlay}_restore")] = restored;
        }
        control.call("action", json!({"name":"minimize"}), deadline)?;
        let minimized = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if minimized["minimized"] != true || minimized["floating"] != true {
            return Err("minimize lost float".into());
        }
        control.call(
            "action",
            json!({"name":"restore-window","window":tiled["id"]}),
            deadline,
        )?;
        let restored = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if restored["geometry"] != moved["geometry"] || restored["floating"] != true {
            return Err("minimize restore lost float rect".into());
        }
        evidence["minimize_restore"] = restored;
        control.call(
            "action",
            json!({"name":"floating","toggle":false}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let retiled = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if retiled["floating"] != false || retiled["geometry"] != tiled["geometry"] {
            return Err(format!("retile did not restore saved tile: {retiled}"));
        }
        evidence["retiled"] = retiled;
        // A committed overlay-sized buffer is not ordinary client size. Enabling
        // floating in either overlay must seed the bounded default float size.
        for overlay in ["maximize", "fullscreen"] {
            control.call("action", json!({"name":overlay,"toggle":true}), deadline)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let full = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
            let extent = [
                full["geometry"]["width"]
                    .as_f64()
                    .ok_or("overlay width absent")?,
                full["geometry"]["height"]
                    .as_f64()
                    .ok_or("overlay height absent")?,
            ];
            state.map_sized(root, [extent[0] as i32, extent[1] as i32], &qh)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            control.call("action", json!({"name":"floating","toggle":true}), deadline)?;
            control.call("action", json!({"name":overlay,"toggle":false}), deadline)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            let seeded = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
            if seeded["floating"] != true
                || seeded["geometry"]["width"].as_f64() != Some(extent[0] * 0.6)
                || seeded["geometry"]["height"].as_f64() != Some(extent[1] * 0.6)
            {
                return Err(format!(
                    "{overlay} committed buffer poisoned floating seed: {seeded}"
                ));
            }
            evidence[format!("floating_seed_from_{overlay}")] = seeded;
            control.call(
                "action",
                json!({"name":"floating","toggle":false}),
                deadline,
            )?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
            state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }
        control.call("action", json!({"name":"floating","toggle":true}), deadline)?;
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if unmapped["mapped"] != false || unmapped["floating"] != false {
            return Err("unmap retained float intent".into());
        }
        evidence["unmapped"] = unmapped;
        let count = state.windows[root].configures;
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "normal remap configure",
            |s| s.windows[root].configures > count,
        )?;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let remapped = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        if remapped["floating"] != false || remapped["mapped"] != true {
            return Err("remap restored stale float".into());
        }
        evidence["remapped"] = remapped;
        // A transient born floating must not reserve a new ordinary tile slot.
        // Keep a later tiled neighbor so an accidental reservation is visible.
        let neighbor = state.create(&qh, "floating-dialog-neighbor");
        state.windows[neighbor].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "dialog neighbor configure",
            |s| s.windows[neighbor].configures > 0,
        )?;
        state.map_sized(neighbor, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "dialog neighbor frame",
            |s| s.windows[neighbor].frames > 0,
        )?;
        control.call(
            "action",
            json!({"name":"focus-window","window":tiled["id"]}),
            deadline,
        )?;
        let parent_before_dialog =
            configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        let neighbor_before_dialog = configure_window(
            &mut control,
            "FluxWM fixture floating-dialog-neighbor",
            deadline,
        )?;
        let child = state.create(&qh, "floating-dialog");
        state.windows[child]
            .toplevel
            .set_parent(Some(&state.windows[root].toplevel));
        state.windows[child].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "dialog configure",
            |s| s.windows[child].configures > 0,
        )?;
        state.map_sized(child, [160, 120], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "dialog frame",
            |s| s.windows[child].frames > 0,
        )?;
        let parent = configure_window(&mut control, tiled["id"].as_str().unwrap(), deadline)?;
        let dialog = configure_window(&mut control, "FluxWM fixture floating-dialog", deadline)?;
        let neighbor_after_dialog = configure_window(
            &mut control,
            neighbor_before_dialog["id"].as_str().unwrap(),
            deadline,
        )?;
        if parent["geometry"] != parent_before_dialog["geometry"]
            || neighbor_after_dialog["geometry"] != neighbor_before_dialog["geometry"]
        {
            return Err(format!(
                "born-floating dialog changed ordinary tile geometry: parent {parent_before_dialog} -> {parent}; neighbor {neighbor_before_dialog} -> {neighbor_after_dialog}"
            ));
        }
        evidence["born_floating_preserves_tiles"] = json!({"parent_before":parent_before_dialog["geometry"],"parent_after":parent["geometry"],"neighbor_before":neighbor_before_dialog["geometry"],"neighbor_after":neighbor_after_dialog["geometry"]});
        if dialog["floating"] != true
            || dialog["geometry"]["width"].as_f64() != Some(160.0)
            || dialog["geometry"]["height"].as_f64() != Some(120.0)
        {
            return Err(format!(
                "same-client parent dialog did not float at actual size: {dialog}"
            ));
        }
        let pg = &parent["geometry"];
        let dg = &dialog["geometry"];
        for (pos, size) in [("x", "width"), ("y", "height")] {
            let expected = pg[pos].as_f64().ok_or("parent position absent")?
                + (pg[size].as_f64().ok_or("parent extent absent")?
                    - dg[size].as_f64().ok_or("dialog extent absent")?)
                    / 2.0;
            if (dg[pos].as_f64().ok_or("dialog position absent")? - expected).abs() > 1.0 {
                return Err(format!("dialog not parent centered: {dialog}"));
            }
        }
        control.call(
            "action",
            json!({"name":"focus-window","window":parent["id"]}),
            deadline,
        )?;
        let exact_parent =
            configure_window(&mut control, parent["id"].as_str().unwrap(), deadline)?;
        if exact_parent["focused"] != true {
            return Err("explicit parent focus failed".into());
        }
        control.call(
            "action",
            json!({"name":"focus-window","window":dialog["id"]}),
            deadline,
        )?;
        let exact_child = configure_window(&mut control, dialog["id"].as_str().unwrap(), deadline)?;
        if exact_child["focused"] != true {
            return Err("explicit floating dialog focus failed".into());
        }
        let malformed = control
            .call(
                "action",
                json!({"name":"focus-window","window":"malformed"}),
                deadline,
            )
            .err()
            .ok_or("malformed focus target accepted")?;
        evidence["explicit_focus"] = json!({"parent":exact_parent["id"],"child":exact_child["id"],"malformed_error":malformed});
        evidence["parent_dialog"] = dialog.clone();
        state.windows[root].toplevel.destroy();
        state.windows[root].xdg.destroy();
        state.windows[root].surface.destroy();
        state.windows[root].destroyed = true;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let survivor = configure_window(&mut control, dialog["id"].as_str().unwrap(), deadline)?;
        if survivor["mapped"] != true
            || survivor["floating"] != true
            || survivor["geometry"] != dialog["geometry"]
        {
            return Err("parent destruction lost/moved surviving floating child".into());
        }
        let stale = control
            .call(
                "action",
                json!({"name":"focus-window","window":parent["id"]}),
                deadline,
            )
            .err()
            .ok_or("destroyed focus target accepted")?;
        let still_focused =
            configure_window(&mut control, dialog["id"].as_str().unwrap(), deadline)?;
        if still_focused["focused"] != true {
            return Err("rejected focus target changed surviving focus".into());
        }
        evidence["stale_focus_error"] = json!(stale);
        evidence["child_survives_parent"] = survivor;
        // A dialog's first retile uses an ordinary workspace slot, including
        // when the workspace has switched to Canvas since its floating birth.
        control.call(
            "action",
            json!({"name":"workspace-mode","mode":"canvas"}),
            deadline,
        )?;
        let ordinary_neighbor = configure_window(
            &mut control,
            neighbor_before_dialog["id"].as_str().unwrap(),
            deadline,
        )?;
        control.call(
            "action",
            json!({"name":"floating","toggle":false}),
            deadline,
        )?;
        let dialog_retiled =
            configure_window(&mut control, dialog["id"].as_str().unwrap(), deadline)?;
        let neighbor_after_retile = configure_window(
            &mut control,
            neighbor_before_dialog["id"].as_str().unwrap(),
            deadline,
        )?;
        let ng = &ordinary_neighbor["geometry"];
        let dg = &dialog_retiled["geometry"];
        if dialog_retiled["floating"] != false
            || dg["width"] != ng["width"]
            || dg["height"] != ng["height"]
            || neighbor_after_retile["geometry"] != ordinary_neighbor["geometry"]
            || dg["x"].as_f64().ok_or("retile position absent")?
                < ng["x"].as_f64().ok_or("neighbor position absent")?
                    + ng["width"].as_f64().ok_or("neighbor width absent")?
        {
            return Err(format!(
                "born-floating Canvas retile reused birth rect or moved ordinary neighbor: {dialog_retiled}; neighbor {ordinary_neighbor} -> {neighbor_after_retile}"
            ));
        }
        evidence["born_floating_canvas_retile"] = json!({"dialog":dialog_retiled,"neighbor_before":ordinary_neighbor["geometry"],"neighbor_after":neighbor_after_retile["geometry"]});
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence
}

/// Real fullscreen wire state, restoration and a stable alpha/letterbox pixel scene.
fn fullscreen(timeout: Duration, pixel_scene: bool, partial: bool) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mode = if partial {
        "fullscreen-pixels-partial"
    } else if pixel_scene {
        "fullscreen-pixels"
    } else {
        "fullscreen"
    };
    let mut evidence = json!({"ok":false,"mode":mode});
    let mut state = State::new(start);
    let mut healthy = State::new(start);
    let result = (|| -> Result<()> {
        let path = display_path()?;
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        if pixel_scene {
            let under = state.create(&qh, "fullscreen-underlay");
            state.windows[under].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "underlay configure",
                |s| s.windows[under].configures > 0,
            )?;
            state.map_sized(under, [WIDTH, HEIGHT], &qh)?;
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "underlay frame",
                |s| s.windows[under].frames > 0,
            )?;
        }
        let root = state.create(&qh, mode);
        state.windows[root].toplevel.set_fullscreen(None);
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "preinitial fullscreen configure",
            |s| s.windows[root].configures > 0,
        )?;
        if !state.windows[root].fullscreen || state.windows[root].maximized {
            return Err("fullscreen wire state missing or accompanied by maximized".into());
        }
        let viewport = state.windows[root].pending_size;
        if viewport[0] <= WIDTH || viewport[1] <= HEIGHT {
            return Err(format!(
                "viewport too small for letterbox fixture: {viewport:?}"
            ));
        }
        if pixel_scene {
            // Deliberately stale client opacity metadata must not suppress the
            // compositor-owned fullscreen backing behind transparent pixels.
            let region = state.compositor.as_ref().unwrap().create_region(&qh, ());
            region.add(0, 0, WIDTH, HEIGHT);
            state.windows[root].surface.set_opaque_region(Some(&region));
            region.destroy();
        }
        state.map_sized_pixels(root, [WIDTH, HEIGHT], pixel_scene, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "undersized fullscreen frame",
            |s| s.windows[root].frames > 0,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if partial {
            diagnostics.call(
                "action",
                json!({"name":"pan","dx":i64::from(viewport[0]) * 3 / 4,"dy":0}),
                deadline,
            )?;
            protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        }
        let first = configure_window(
            &mut diagnostics,
            &format!("FluxWM fixture {mode}"),
            deadline,
        )?;
        if first["fullscreen"] != true {
            return Err("initial model not fullscreen".into());
        }
        evidence["preinitial"] = first.clone();
        evidence["viewport"] = json!(viewport);
        evidence["partial_page"] = json!(partial);
        evidence["client_opaque_region_claim"] = json!(pixel_scene);
        evidence["client_size"] = json!([WIDTH, HEIGHT]);
        evidence["capabilities"] = json!(state.windows[root].capability_values);
        if pixel_scene {
            evidence["ready_at_ms"] = json!(start.elapsed().as_millis());
            observe_for(
                &connection,
                &mut queue,
                &mut state,
                Duration::from_millis(1000),
                deadline,
            )?;
            let settled =
                configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
            if settled != first {
                return Err("fullscreen pixel scene changed during screenshot interval".into());
            }
            evidence["after_settle"] = settled;
            evidence["transparent_right_half"] = json!(true);
            evidence["colored_underlay_present"] = json!(true);
            return Ok(());
        }
        let count = state.windows[root].configures;
        state.windows[root].toplevel.set_fullscreen(None);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "repeated fullscreen response",
            |s| s.windows[root].configures > count,
        )?;
        evidence["repeated_fullscreen_replied"] = json!(true);
        state.windows[root].toplevel.unset_fullscreen();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.windows[root].fullscreen {
            return Err("unset fullscreen wire state remained".into());
        }
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let normal = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        evidence["normal"] = normal.clone();
        diagnostics.call("action", json!({"name":"maximize","toggle":true}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let size = state.windows[root].pending_size;
        state.map_sized(root, size, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        diagnostics.call(
            "action",
            json!({"name":"fullscreen","toggle":true}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let overlay = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if overlay["fullscreen"] != true
            || overlay["maximized"] != true
            || !state.windows[root].fullscreen
            || state.windows[root].maximized
        {
            return Err(
                "fullscreen did not preserve model maximize intent while suppressing wire maximize"
                    .into(),
            );
        }
        evidence["maximized_fullscreen"] = overlay;
        state.windows[root].ack_configures = false;
        let before = state.windows[root].configures;
        state.windows[root].toplevel.set_fullscreen(None);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "held fullscreen response",
            |s| s.windows[root].configures > before,
        )?;
        for _ in 0..16 {
            state.windows[root].toplevel.unset_fullscreen();
            state.windows[root].toplevel.set_fullscreen(None);
        }
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let bounded = state.windows[root].configures - before;
        if bounded != 1 {
            return Err(format!(
                "fullscreen replies escaped single-flight bound: {bounded}"
            ));
        }
        let held = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].ack_configures = true;
        state.windows[root].xdg.ack_configure(held);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "coalesced fullscreen reply",
            |s| s.windows[root].configures > before + 1,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        evidence["held_reply_bound"] = json!(bounded);
        state.windows[root].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let hidden = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if hidden["minimized"] != true || hidden["fullscreen"] != true {
            return Err("minimize lost fullscreen".into());
        }
        diagnostics.call(
            "action",
            json!({"name":"restore-window","window":first["id"]}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let restored = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if restored["minimized"] != false
            || restored["fullscreen"] != true
            || restored["focused"] != true
        {
            return Err("restore lost fullscreen/focus".into());
        }
        evidence["minimize_restore"] = restored;
        diagnostics.call(
            "action",
            json!({"name":"fullscreen","toggle":false}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.windows[root].fullscreen || !state.windows[root].maximized {
            return Err("unfullscreen did not restore maximized wire state".into());
        }
        let size = state.windows[root].pending_size;
        state.map_sized(root, size, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        diagnostics.call(
            "action",
            json!({"name":"maximize","toggle":false}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let normal_again =
            configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if normal_again["geometry"] != normal["geometry"]
            || normal_again["fullscreen"] != false
            || normal_again["maximized"] != false
        {
            return Err("fullscreen/maximize did not restore normal base geometry".into());
        }
        evidence["normal_restored"] = normal_again;
        diagnostics.call("action", json!({"name":"maximize","toggle":true}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[root].toplevel.set_fullscreen(None);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let before_unmap =
            configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if !state.windows[root].fullscreen
            || state.windows[root].maximized
            || before_unmap["fullscreen"] != true
            || before_unmap["maximized"] != true
        {
            return Err(
                "unmap precondition lacks fullscreen wire state and maximize restore intent".into(),
            );
        }
        evidence["fullscreen_maximize_before_unmap"] = before_unmap;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if unmapped["mapped"] != false
            || unmapped["fullscreen"] != false
            || unmapped["maximized"] != false
        {
            return Err("unmap retained fullscreen".into());
        }
        evidence["unmapped"] = unmapped;
        let configure_count = state.windows[root].configures;
        state.windows[root]
            .toplevel
            .set_title("FluxWM fixture fullscreen-remapped".into());
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "normal configure after fullscreen unmap",
            |s| s.windows[root].configures > configure_count,
        )?;
        if state.windows[root].fullscreen || state.windows[root].maximized {
            return Err(
                "remap initial configure retained fullscreen or maximized wire state".into(),
            );
        }
        let frames = state.windows[root].frames;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "normal remap frame",
            |s| s.windows[root].frames > frames,
        )?;
        evidence["remap_wire_states_cleared"] = json!(true);
        let second = state.create(&qh, "fullscreen-before-map");
        state.windows[second].ack_configures = false;
        state.windows[second].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "held initial configure",
            |s| s.windows[second].configures > 0,
        )?;
        state.windows[second].toplevel.set_fullscreen(None);
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let serial = *state.windows[second].configure_serials.last().unwrap();
        state.windows[second].ack_configures = true;
        state.windows[second].xdg.ack_configure(serial);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "fullscreen before first buffer reply",
            |s| s.windows[second].configures > 1 && s.windows[second].fullscreen,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.map_sized(second, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "deferred fullscreen frame",
            |s| s.windows[second].frames > 0,
        )?;
        evidence["pre_map_deferred_reply"] = json!(true);
        let healthy_connection = connect(&path, deadline)?;
        let mut healthy_queue = healthy_connection.new_event_queue::<State>();
        let healthy_qh = healthy_queue.handle();
        health_registry(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let survivor = healthy.create(&healthy_qh, "fullscreen-survivor");
        healthy.windows[survivor].surface.commit();
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "survivor configure",
            |s| s.windows[survivor].configures > 0,
        )?;
        healthy.map(survivor, &healthy_qh)?;
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "survivor frame",
            |s| s.windows[survivor].frames > 0,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let background = configure_window(
            &mut diagnostics,
            "FluxWM fixture fullscreen-before-map",
            deadline,
        )?;
        if background["fullscreen"] != true || background["focused"] != false {
            return Err("focus change cleared fullscreen".into());
        }
        evidence["focus_away_preserves_fullscreen"] = background;
        let size = state.windows[second].pending_size;
        state.map_sized(second, [size[0] + 1, size[1]], &qh)?;
        let rejected = protocol_roundtrip(&connection, &mut queue, &mut state, deadline)
            .err()
            .ok_or("oversized fullscreen commit accepted")?;
        let error = connection
            .protocol_error()
            .ok_or_else(|| format!("missing oversize protocol error: {rejected}"))?;
        if error.code != xdg_wm_base::Error::InvalidSurfaceState as u32
            || error.object_interface != "xdg_wm_base"
        {
            return Err(format!("wrong fullscreen geometry error: {error:?}"));
        }
        evidence["malformed_geometry_error"] =
            json!({"code":error.code,"interface":error.object_interface,"message":error.message});
        protocol_roundtrip(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let alive = configure_window(
            &mut diagnostics,
            "FluxWM fixture fullscreen-survivor",
            deadline,
        )?;
        if alive["mapped"] != true {
            return Err("oversize rejection lost healthy sibling".into());
        }
        evidence["healthy_survivor"] = alive;
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence["healthy_events"] = json!(healthy.events);
    evidence
}

/// Real wire maximize requests, bounded configure replies and exact-size commits.
fn maximize(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.popup_mode = true;
    state.pointer_feedback_enabled = true;
    let mut healthy = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"maximize"});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        let root = state.create(&qh, "maximize");
        state.windows[root].toplevel.set_maximized();
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "preinitial maximize configure",
            |s| s.windows[root].configures > 0,
        )?;
        if !state.windows[root].maximized {
            return Err("preinitial maximize missing wire state".into());
        }
        let size = state.windows[root].pending_size;
        state.map_sized(root, size, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "preinitial maximized frame",
            |s| s.windows[root].frames > 0,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let first = configure_window(&mut diagnostics, "FluxWM fixture maximize", deadline)?;
        if first["maximized"] != true {
            return Err("initial model not maximized".into());
        }
        evidence["preinitial"] = first.clone();
        // A repeated request still needs a configure even when its state is unchanged.
        let count = state.windows[root].configures;
        state.windows[root].toplevel.set_maximized();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "same-state maximize response",
            |s| s.windows[root].configures > count,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        evidence["repeated_maximize_replied"] = json!(true);
        state.windows[root].toplevel.unset_maximized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.windows[root].maximized {
            return Err("unmaximize wire state remained set".into());
        }
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let normal = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if normal["maximized"] != false {
            return Err("unmaximize model remained set".into());
        }
        evidence["normal"] = normal.clone();
        let count = state.windows[root].configures;
        state.windows[root].toplevel.unset_maximized();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "same-state unmaximize response",
            |s| s.windows[root].configures > count,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        // Hold the only in-flight configure. Old normal commits are legal before ack.
        state.windows[root].ack_configures = false;
        state.windows[root].toplevel.set_maximized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let held = state.windows[root].configures;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        for _ in 0..12 {
            state.windows[root].toplevel.unset_maximized();
            state.windows[root].toplevel.set_maximized();
        }
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let bounded = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if bounded["configure"]["unacknowledged"] != 1 || state.windows[root].configures != held {
            return Err(format!(
                "maximize replies escaped single-flight bound: {bounded}"
            ));
        }
        evidence["held_ack"] = bounded;
        let serial = *state.windows[root].configure_serials.last().unwrap();
        state.windows[root].ack_configures = true;
        state.windows[root].xdg.ack_configure(serial);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "coalesced maximize response",
            |s| s.windows[root].configures > held,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if !state.windows[root].maximized {
            return Err("coalesced latest maximize state missing".into());
        }
        let size = state.windows[root].pending_size;
        state.map_sized(root, size, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let maximized =
            configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if maximized["maximized"] != true || maximized["id"] != normal["id"] {
            return Err("mapped maximize identity/state failed".into());
        }
        evidence["maximized"] = maximized;
        state.windows[root].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let hidden = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if hidden["minimized"] != true || hidden["maximized"] != true {
            return Err("minimize lost maximize state".into());
        }
        diagnostics.call(
            "action",
            json!({"name":"restore-window","window":first["id"]}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let restored = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if restored["minimized"] != false
            || restored["maximized"] != true
            || restored["focused"] != true
        {
            return Err("restore did not preserve maximization/focus".into());
        }
        evidence["minimize_restore"] = restored;
        diagnostics.call(
            "action",
            json!({"name":"maximize","toggle":false}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.map_sized(root, [WIDTH, HEIGHT], &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let normal_again =
            configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if normal_again["geometry"] != normal["geometry"] || normal_again["maximized"] != false {
            return Err("unmaximize did not restore saved normal geometry".into());
        }
        evidence["normal_restored"] = normal_again;
        diagnostics.call("action", json!({"name":"maximize","toggle":true}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let size = state.windows[root].pending_size;
        state.map_sized(root, size, &qh)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[root].surface.attach(None, 0, 0);
        state.windows[root].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut diagnostics, first["id"].as_str().unwrap(), deadline)?;
        if unmapped["maximized"] != false || unmapped["mapped"] != false {
            return Err("unmap retained maximization".into());
        }
        evidence["unmapped"] = unmapped;
        // Request while the initial configure is held, before the first buffer.
        let second = state.create(&qh, "maximize-before-map");
        state.windows[second].ack_configures = false;
        state.windows[second].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "held initial configure",
            |s| s.windows[second].configures > 0,
        )?;
        state.windows[second].toplevel.set_maximized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let serial = *state.windows[second].configure_serials.last().unwrap();
        state.windows[second].ack_configures = true;
        state.windows[second].xdg.ack_configure(serial);
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "maximize response before first buffer",
            |s| s.windows[second].configures > 1 && s.windows[second].maximized,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let size = state.windows[second].pending_size;
        state.map_sized(second, size, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "deferred initial maximize frame",
            |s| s.windows[second].frames > 0,
        )?;
        evidence["pre_map_deferred_reply"] = json!(true);
        // Remap the older role as a later column, then focus the earlier
        // maximized peer. Both fill the viewport, so input at the compositor's
        // unchanged (0,0) pointer proves which surface is actually on top.
        let count = state.windows[root].configures;
        state.windows[root]
            .toplevel
            .set_title("FluxWM fixture maximize-peer".into());
        state.windows[root].toplevel.set_maximized();
        state.windows[root].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "peer remap configure",
            |s| s.windows[root].configures > count && s.windows[root].maximized,
        )?;
        let peer_size = state.windows[root].pending_size;
        state.map_sized(root, peer_size, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "later overlay pointer precondition",
            |s| {
                s.pointer_focus_surface.as_ref() == Some(&s.windows[root].surface)
                    && s.keyboard_focus.as_ref() == Some(&s.windows[root].surface)
            },
        )?;
        diagnostics.call(
            "action",
            json!({"name":"focus","direction":"left"}),
            deadline,
        )?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "earlier maximized peer raised",
            |s| {
                s.pointer_focus_surface.as_ref() == Some(&s.windows[second].surface)
                    && s.keyboard_focus.as_ref() == Some(&s.windows[second].surface)
            },
        )?;
        evidence["focus_stacking"] =
            json!({"later_pointer_precondition":true,"earlier_pointer_and_keyboard":true});

        // A fixed-size client cannot fill the viewport. Decline its request
        // with a configure rather than demand an impossible buffer size.
        let fixed = state.create(&qh, "maximize-fixed-size");
        state.windows[fixed].toplevel.set_min_size(WIDTH, HEIGHT);
        state.windows[fixed].toplevel.set_max_size(WIDTH, HEIGHT);
        state.windows[fixed].toplevel.set_maximized();
        state.windows[fixed].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "fixed-size preinitial configure",
            |s| s.windows[fixed].configures > 0,
        )?;
        if state.windows[fixed].maximized {
            return Err("fixed-size preinitial request advertised impossible maximization".into());
        }
        state.map_sized(fixed, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "fixed-size normal frame",
            |s| s.windows[fixed].frames > 0,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let count = state.windows[fixed].configures;
        state.windows[fixed].toplevel.set_maximized();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "fixed-size declined maximize reply",
            |s| s.windows[fixed].configures > count,
        )?;
        if state.windows[fixed].maximized {
            return Err("fixed-size mapped request advertised impossible maximization".into());
        }
        let frames = state.windows[fixed].frames;
        state.map_sized(fixed, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "declined maximize normal commit survival",
            |s| s.windows[fixed].frames > frames,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let before_rejection = configure_window(
            &mut diagnostics,
            "FluxWM fixture maximize-fixed-size",
            deadline,
        )?;
        if before_rejection["maximized"] != false || before_rejection["focused"] != true {
            return Err("fixed-size IPC rejection precondition failed".into());
        }
        let ipc_rejection = diagnostics
            .call(
                "action",
                json!({"name":"maximize", "toggle":true}),
                deadline,
            )
            .err()
            .ok_or("IPC accepted impossible fixed-size maximization")?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let fixed_alive = configure_window(
            &mut diagnostics,
            "FluxWM fixture maximize-fixed-size",
            deadline,
        )?;
        if fixed_alive["mapped"] != true
            || fixed_alive["maximized"] != false
            || fixed_alive["geometry"] != before_rejection["geometry"]
            || !state.windows[second].maximized
            || !state.windows[root].maximized
        {
            return Err(
                "declined fixed-size request changed client mapping, geometry or peers".into(),
            );
        }
        evidence["fixed_size_declined"] = json!({
            "preinitial_and_mapped_wire_replies":true,
            "normal_commit_survived":true,
            "ipc_error":ipc_rejection,
            "window":fixed_alive,
        });

        // Constraints committed with the first buffer can invalidate the
        // earlier maximize promise. Admit a visible normal window and send
        // its correction instead of leaving the role permanently unmapped.
        let late_fixed = state.create(&qh, "maximize-late-fixed-size");
        state.windows[late_fixed].toplevel.set_maximized();
        state.windows[late_fixed].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "late fixed-size initial maximized configure",
            |s| s.windows[late_fixed].configures > 0,
        )?;
        if !state.windows[late_fixed].maximized {
            return Err("late fixed-size case lacks initial maximize promise".into());
        }
        let promised_size = state.windows[late_fixed].pending_size;
        state.windows[late_fixed]
            .toplevel
            .set_min_size(WIDTH, HEIGHT);
        state.windows[late_fixed]
            .toplevel
            .set_max_size(WIDTH, HEIGHT);
        state.map_sized(late_fixed, promised_size, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "late fixed-size first frame and normal correction",
            |s| s.windows[late_fixed].frames > 0 && !s.windows[late_fixed].maximized,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let late_admitted = configure_window(
            &mut diagnostics,
            "FluxWM fixture maximize-late-fixed-size",
            deadline,
        )?;
        if late_admitted["mapped"] != true || late_admitted["maximized"] != false {
            return Err("constraints at first buffer left window unmapped or maximized".into());
        }
        let frames = state.windows[late_fixed].frames;
        state.map_sized(late_fixed, [WIDTH, HEIGHT], &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "late fixed-size corrected normal frame",
            |s| s.windows[late_fixed].frames > frames,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        evidence["late_fixed_size_admitted"] = json!({
            "promised_size":promised_size,
            "window":late_admitted,
            "corrected_normal_frame":true,
        });

        // Malformed geometry must retire this connection, not a separate healthy client.
        let healthy_connection = connect(&path, deadline)?;
        let mut healthy_queue = healthy_connection.new_event_queue::<State>();
        let healthy_qh = healthy_queue.handle();
        health_registry(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let survivor = healthy.create(&healthy_qh, "maximize-survivor");
        healthy.windows[survivor].surface.commit();
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "survivor configure",
            |s| s.windows[survivor].configures > 0,
        )?;
        healthy.map(survivor, &healthy_qh)?;
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "survivor frame",
            |s| s.windows[survivor].frames > 0,
        )?;
        state.map_sized(second, [size[0] - 1, size[1]], &qh)?;
        let rejected = protocol_roundtrip(&connection, &mut queue, &mut state, deadline)
            .err()
            .ok_or("wrong-sized maximized commit accepted")?;
        let error = connection
            .protocol_error()
            .ok_or_else(|| format!("missing protocol error after rejection: {rejected}"))?;
        if error.code != xdg_wm_base::Error::InvalidSurfaceState as u32
            || error.object_interface != "xdg_wm_base"
        {
            return Err(format!("wrong geometry error: {error:?}"));
        }
        evidence["malformed_geometry_error"] =
            json!({"code":error.code,"interface":error.object_interface,"message":error.message});
        protocol_roundtrip(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let survivor_state = configure_window(
            &mut diagnostics,
            "FluxWM fixture maximize-survivor",
            deadline,
        )?;
        if survivor_state["mapped"] != true {
            return Err("healthy sibling lost mapping".into());
        }
        let stale = diagnostics
            .call(
                "action",
                json!({"name":"restore-window","window":first["id"]}),
                deadline,
            )
            .err()
            .ok_or("retired ID accepted restore")?;
        if !stale.contains("unknown or unmapped window") {
            return Err(format!("unexpected stale ID response: {stale}"));
        }
        evidence["stale_restore_error"] = json!(stale);
        evidence["healthy_survivor"] = survivor_state;
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence["healthy_events"] = json!(healthy.events);
    evidence
}

/// Exercise real XDG minimize requests and the compositor's explicit restore API.
/// Refusing close is legal; explicit disconnection targets the exact connection.
fn close_client(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    let mut healthy = State::new(start);
    let mut evidence = json!({"ok":false,"mode":"close"});
    let result = (|| -> Result<()> {
        let path = display_path()?;
        let connection = connect(&path, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        for name in [
            "close-target",
            "close-hidden-sibling",
            "close-unmapped-sibling",
        ] {
            let index = state.create(&qh, name);
            state.windows[index].allow_close = true;
            state.windows[index].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "close initial configure",
                |s| s.windows[index].configures > 0,
            )?;
            if index < 2 {
                state.map(index, &qh)?;
                until(
                    &connection,
                    &mut queue,
                    &mut state,
                    deadline,
                    "close initial frame",
                    |s| s.windows[index].frames > 0,
                )?;
            }
        }
        state.windows[1].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let victim = configure_window(&mut diagnostics, "FluxWM fixture close-target", deadline)?;
        let hidden = configure_window(
            &mut diagnostics,
            "FluxWM fixture close-hidden-sibling",
            deadline,
        )?;
        let unmapped = configure_window(
            &mut diagnostics,
            "FluxWM fixture close-unmapped-sibling",
            deadline,
        )?;
        if victim["focused"] != true || hidden["minimized"] != true || unmapped["mapped"] != false {
            return Err("close connection membership preconditions failed".into());
        }
        diagnostics.call("action", json!({"name":"close-window"}), deadline)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "real close event",
            |s| s.windows[0].closes > 0,
        )?;
        let refused = configure_window(&mut diagnostics, victim["id"].as_str().unwrap(), deadline)?;
        if refused["mapped"] != true
            || state.windows[0].closes != 1
            || state.windows[1].closes != 0
            || state.windows[2].closes != 0
        {
            return Err("graceful close changed mapping or reached an unfocused sibling".into());
        }
        let frames = state.windows[0].frames;
        state.map(0, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "refusing client frame",
            |s| s.windows[0].frames > frames,
        )?;
        diagnostics.call(
            "action",
            json!({"name":"restore-window","window":hidden["id"]}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        diagnostics.call(
            "action",
            json!({"name":"focus","direction":"left"}),
            deadline,
        )?;
        if configure_window(&mut diagnostics, victim["id"].as_str().unwrap(), deadline)?["focused"]
            != true
        {
            return Err("refused close prevented navigation".into());
        }
        state.windows[1].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        evidence["close_refused"] = json!({"mapped":refused,"close_events":state.windows[0].closes,
            "frames_before":frames,"frames_after":state.windows[0].frames,"navigation_continues":true});
        let healthy_connection = connect(&path, deadline)?;
        let mut healthy_queue = healthy_connection.new_event_queue::<State>();
        let healthy_qh = healthy_queue.handle();
        health_registry(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let survivor = healthy.create(&healthy_qh, "close-independent");
        healthy.windows[survivor].surface.commit();
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "independent configure",
            |s| s.windows[survivor].configures > 0,
        )?;
        healthy.map(survivor, &healthy_qh)?;
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "independent frame",
            |s| s.windows[survivor].frames > 0,
        )?;
        let independent = configure_window(
            &mut diagnostics,
            "FluxWM fixture close-independent",
            deadline,
        )?;
        if victim["app_id"] != "org.fluxwm.Fixture" || independent["app_id"] != "org.fluxwm.Fixture"
        {
            return Err("same app-id isolation precondition failed".into());
        }
        // Deliberately target the live UNMAPPED role, never process metadata or focus.
        let accepted = diagnostics.call(
            "action",
            json!({"name":"disconnect-client","window":unmapped["id"]}),
            deadline,
        )?;
        let mut affected = accepted["affected_windows"]
            .as_array()
            .ok_or("missing affected_windows")?
            .clone();
        let mut expected = vec![
            victim["id"].clone(),
            hidden["id"].clone(),
            unmapped["id"].clone(),
        ];
        affected.sort_by_key(Json::to_string);
        expected.sort_by_key(Json::to_string);
        if accepted["accepted"] != true
            || accepted["scope"] != "wayland-client-connection"
            || accepted["other_windows_may_be_lost"] != true
            || affected != expected
        {
            return Err(format!(
                "wrong disconnection acceptance/membership: {accepted}"
            ));
        }
        evidence["accepted"] = accepted;
        // No victim flush/request or healthy-peer traffic may drive cleanup.
        // POLLHUP is reported even without requested events; queued configure
        // bytes cannot turn this into a false readiness success.
        let silence_start = Instant::now();
        let quiet_peer_events = poll_fd(
            connection.as_fd().as_raw_fd(),
            0,
            deadline.min(silence_start + Duration::from_millis(350)),
        )
        .map_err(|error| format!("silent victim did not disconnect: {error}"))?;
        if quiet_peer_events & libc::POLLHUP == 0 {
            return Err(format!(
                "silent victim lacked peer hangup: {quiet_peer_events}"
            ));
        }
        evidence["silent_peer_hangup"] = json!({"no_requests_or_flush_after_acceptance":true,
            "poll_events":quiet_peer_events,"elapsed_ms":silence_start.elapsed().as_millis()});
        let disconnected = protocol_roundtrip(&connection, &mut queue, &mut state, deadline)
            .err()
            .ok_or("victim connection still answers after disconnect")?;
        let mut peer = libc::pollfd {
            fd: connection.as_fd().as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        // Verify an actual transport failure, never accept a fixture timeout.
        let polled = unsafe { libc::poll(&mut peer, 1, 0) };
        let transport_failed = matches!(connection.backend().last_error(),
            Some(wayland_client::backend::WaylandError::Io(ref error))
                if matches!(error.kind(), std::io::ErrorKind::BrokenPipe
                    | std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::UnexpectedEof));
        if connection.protocol_error().is_some()
            || !(transport_failed || (polled == 1 && peer.revents & libc::POLLHUP != 0))
        {
            return Err(format!("disconnect lacked transport EOF: {disconnected}"));
        }
        evidence["victim_connection_eof"] = json!({"error":disconnected,"poll_hangup":peer.revents & libc::POLLHUP != 0,
            "transport_error":transport_failed});
        protocol_roundtrip(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
        )?;
        let after = diagnostics.call("windows.list", json!({}), deadline)?;
        let remaining = after["windows"]
            .as_array()
            .ok_or("missing windows after disconnection")?;
        if remaining.len() != 1
            || remaining[0]["id"] != independent["id"]
            || remaining[0]["mapped"] != true
        {
            return Err(format!(
                "wrong client removed or connection roles survived: {after}"
            ));
        }
        evidence["after_disconnect"] = after;
        let before_sequence = diagnostics.last_sequence;
        let stale = diagnostics
            .call(
                "action",
                json!({"name":"disconnect-client","window":victim["id"]}),
                deadline,
            )
            .err()
            .ok_or("stale disconnect target accepted")?;
        if diagnostics.last_sequence != before_sequence
            || !stale.contains("\"code\":\"not_ready\"")
            || !stale.contains("window is not a live toplevel")
        {
            return Err(format!(
                "stale disconnect lacked unchanged-sequence target rejection: {stale}"
            ));
        }
        evidence["stale_rejection"] = json!({"error":stale,"sequence_unchanged":true});
        let healthy_frames = healthy.windows[survivor].frames;
        healthy.map(survivor, &healthy_qh)?;
        until(
            &healthy_connection,
            &mut healthy_queue,
            &mut healthy,
            deadline,
            "survivor frame after disconnection",
            |s| s.windows[survivor].frames > healthy_frames,
        )?;
        evidence["independent"] = json!({"before":independent,"after":configure_window(&mut diagnostics,
            independent["id"].as_str().unwrap(),deadline)?,"frames_before":healthy_frames,
            "frames_after":healthy.windows[survivor].frames,"same_process":true,"same_app_id":true});
        Ok(())
    })();
    evidence["ok"] = json!(result.is_ok());
    evidence["error"] = json!(result.err());
    evidence["elapsed_ms"] = json!(start.elapsed().as_millis());
    evidence["events"] = json!(state.events);
    evidence["healthy_events"] = json!(healthy.events);
    evidence
}

fn minimize(timeout: Duration) -> Json {
    let start = Instant::now();
    let deadline = start + timeout;
    let mut state = State::new(start);
    state.popup_mode = true; // Bind a real keyboard and observe focus events.
    state.pointer_feedback_enabled = true;
    let mut evidence = json!({"ok":false,"mode":"minimize"});
    let result = (|| -> Result<()> {
        let connection = connect(&display_path()?, deadline)?;
        let mut queue = connection.new_event_queue::<State>();
        let qh = queue.handle();
        health_registry(&connection, &mut queue, &mut state, deadline)?;
        let ipc = std::env::var_os("FLUXWM_SOCKET").ok_or("explicit IPC socket required")?;
        let mut diagnostics = Diagnostics::new(Path::new(&ipc), deadline)?;
        for name in ["minimize-A", "minimize-B"] {
            let index = state.create(&qh, name);
            state.windows[index].surface.commit();
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "initial configure",
                |s| s.windows[index].configures > 0,
            )?;
            state.map(index, &qh)?;
            until(
                &connection,
                &mut queue,
                &mut state,
                deadline,
                "initial visible frame",
                |s| s.windows[index].frames > 0,
            )?;
        }
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.windows.iter().any(|w| {
            w.toplevel.version() < 5
                || w.capabilities == 0
                || w.capability_values
                    != [
                        xdg_toplevel::WmCapabilities::Maximize as u32,
                        xdg_toplevel::WmCapabilities::Fullscreen as u32,
                        xdg_toplevel::WmCapabilities::Minimize as u32,
                    ]
        }) {
            return Err("missing actual protocol v5+ Minimize capability".into());
        }
        let a = configure_window(&mut diagnostics, "FluxWM fixture minimize-A", deadline)?;
        let b = configure_window(&mut diagnostics, "FluxWM fixture minimize-B", deadline)?;
        if a["focused"] != false || b["focused"] != true || b["minimized"] != false {
            return Err("initial focus/minimize precondition failed".into());
        }
        if state.keyboard.is_none()
            || state.keyboard_focus.as_ref() != Some(&state.windows[1].surface)
        {
            return Err("initial real keyboard focus missing".into());
        }
        evidence["before"] = json!([a.clone(), b.clone()]);
        // A client request targets its own toplevel, even while B owns focus.
        state.windows[0].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let hidden_a = configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?;
        let still_b = configure_window(&mut diagnostics, b["id"].as_str().unwrap(), deadline)?;
        if hidden_a["minimized"] != true
            || hidden_a["mapped"] != true
            || hidden_a["focused"] != false
            || still_b["focused"] != true
            || hidden_a["geometry"] != a["geometry"]
        {
            return Err("background minimize changed focus, mapping or reserved geometry".into());
        }
        evidence["background_minimize"] = hidden_a;
        diagnostics.call(
            "action",
            json!({"name":"restore-window", "window":a["id"]}),
            deadline,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let restored_a = configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?;
        if restored_a["minimized"] != false
            || restored_a["focused"] != true
            || restored_a["geometry"] != a["geometry"]
            || restored_a["id"] != a["id"]
        {
            return Err("restore lost identity, focus or geometry".into());
        }
        evidence["restored"] = restored_a;
        let popup = state.create_popup(0, false, &qh);
        state.popups[popup].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup configure",
            |s| s.popups[popup].configures > 0,
        )?;
        state.map_popup(popup, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "popup visible frame",
            |s| s.popups[popup].frames > 0,
        )?;
        // Focused IPC minimize must choose the remaining visible window.
        diagnostics.call("action", json!({"name":"minimize"}), deadline)?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let fallback = diagnostics.call("windows.list", json!({}), deadline)?;
        if configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?["minimized"]
            != true
            || configure_window(&mut diagnostics, b["id"].as_str().unwrap(), deadline)?["focused"]
                != true
            || state.windows[0].activated
        {
            return Err("focused minimize failed focus fallback/deactivation".into());
        }
        evidence["focus_fallback"] = fallback;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "minimized popup dismissal",
            |s| s.popups[popup].done > 0,
        )?;
        if state.popups[popup].done != 1 {
            return Err("minimize popup dismissal count differed from one".into());
        }
        evidence["mapped_popup_dismissed"] =
            json!({"frames":state.popups[popup].frames,"done":state.popups[popup].done});
        state.destroy_popup(popup);
        // A new buffer and title cannot silently unminimize a hidden owner.
        let frames = state.windows[0].frames;
        state.map(0, &qh)?;
        state.windows[0]
            .toplevel
            .set_title("FluxWM fixture minimize-A-committed".into());
        state.windows[0].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        observe_for(
            &connection,
            &mut queue,
            &mut state,
            Duration::from_millis(120),
            deadline,
        )?;
        let hidden_commit =
            configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?;
        if hidden_commit["minimized"] != true
            || hidden_commit["mapped"] != true
            || state.windows[0].frames != frames
        {
            return Err("hidden commit unminimized or received a presentation frame".into());
        }
        evidence["hidden_commit"] = hidden_commit;
        evidence["hidden_frame_callbacks_before_after"] = json!([frames, state.windows[0].frames]);
        state.windows[1].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let all_hidden = diagnostics.call("windows.list", json!({}), deadline)?;
        if !all_hidden["windows"]
            .as_array()
            .ok_or("missing windows")?
            .iter()
            .all(|w| w["minimized"] == true && w["focused"] == false && w["mapped"] == true)
            || state.windows.iter().any(|w| w.activated || !w.suspended)
            || state.keyboard_focus.is_some()
            || state.pointer_focus_surface.is_some()
        {
            return Err("minimize-all retained focus or activation".into());
        }
        evidence["all_hidden"] = all_hidden;
        diagnostics.call(
            "action",
            json!({"name":"restore-window", "window":a["id"]}),
            deadline,
        )?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "restored frame",
            |s| s.windows[0].frames > frames,
        )?;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        if state.keyboard_focus.as_ref() != Some(&state.windows[0].surface)
            || !state.windows[0].activated
            || state.windows[0].suspended
        {
            return Err("restore did not recover keyboard focus and active wire state".into());
        }
        evidence["restore_focus_and_wire_state"] = json!({"activated":state.windows[0].activated,
            "suspended":state.windows[0].suspended,"keyboard_focus":true});
        // Unmap/remap resets minimized state on the same XDG role/identity.
        state.windows[0].toplevel.set_minimized();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        state.windows[0].surface.attach(None, 0, 0);
        state.windows[0].surface.commit();
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let unmapped = configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?;
        if unmapped["mapped"] != false || unmapped["minimized"] != false {
            return Err("unmap retained minimized state".into());
        }
        evidence["unmapped"] = unmapped;
        let count = state.windows[0].configures;
        state.windows[0].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "remap configure",
            |s| s.windows[0].configures > count,
        )?;
        let frames = state.windows[0].frames;
        state.map(0, &qh)?;
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "remap visible frame",
            |s| s.windows[0].frames > frames,
        )?;
        let remapped = configure_window(&mut diagnostics, a["id"].as_str().unwrap(), deadline)?;
        if remapped["id"] != a["id"] || remapped["mapped"] != true || remapped["minimized"] != false
        {
            return Err("remap did not reset visibility on same identity".into());
        }
        evidence["remapped"] = remapped;
        state.windows[0].toplevel.destroy();
        state.windows[0].xdg.destroy();
        state.windows[0].surface.destroy();
        state.windows[0].destroyed = true;
        protocol_roundtrip(&connection, &mut queue, &mut state, deadline)?;
        let replacement = state.create(&qh, "minimize-replacement");
        state.windows[replacement].surface.commit();
        until(
            &connection,
            &mut queue,
            &mut state,
            deadline,
            "replacement configure",
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
            "FluxWM fixture minimize-replacement",
            deadline,
        )?;
        let old_id = a["id"].as_str().ok_or("missing old ID")?;
        let new_id = new["id"].as_str().ok_or("missing replacement ID")?;
        if old_id == new_id
            || old_id.split_once(':').map(|p| p.0) != new_id.split_once(':').map(|p| p.0)
        {
            return Err("stale-ID test did not exercise actual slot reuse".into());
        }
        evidence["replacement"] = new;
        let stale = diagnostics
            .call(
                "action",
                json!({"name":"restore-window", "window":a["id"]}),
                deadline,
            )
            .err()
            .ok_or("destroyed ID unexpectedly accepted restore")?;
        if !stale.contains("unknown or unmapped window") {
            return Err(format!("unexpected stale restore error: {stale}"));
        }
        evidence["stale_restore_error"] = json!(stale);
        evidence["protocol_capability_values"] = json!(
            state
                .windows
                .iter()
                .map(|w| w.capability_values.clone())
                .collect::<Vec<_>>()
        );
        evidence["visible_frames"] =
            json!(state.windows.iter().map(|w| w.frames).collect::<Vec<_>>());
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
                || mode == "configure-admission"
                || mode == "close"
                || mode == "minimize"
                || mode == "maximize"
                || mode == "floating-pixels"
                || mode == "floating-pixels-fullscreen"
                || mode == "floating"
                || mode == "fullscreen"
                || mode == "fullscreen-pixels"
                || mode == "fullscreen-pixels-partial" =>
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
                || mode == "configure-admission"
                || mode == "close"
                || mode == "minimize"
                || mode == "maximize"
                || mode == "floating-pixels"
                || mode == "floating-pixels-fullscreen"
                || mode == "floating"
                || mode == "fullscreen"
                || mode == "fullscreen-pixels"
                || mode == "fullscreen-pixels-partial")
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
            "usage: fluxwm-fixture lifecycle|close|minimize|maximize|floating|fullscreen|fullscreen-pixels|fullscreen-pixels-partial|config-width|config-geometry|configure-stream|configure-admission|slow-reader|pointer-feedback|popup-lifecycle|container-clip|popup-clip|input-probe|shell-health|shell-health-capacity|shell-health-disconnect [--timeout-ms N; maximum depends on mode]",
        ),
    };
    let result = match parsed {
        Ok(("close", timeout)) => close_client(timeout),
        Ok(("minimize", timeout)) => minimize(timeout),
        Ok(("maximize", timeout)) => maximize(timeout),
        Ok(("floating-pixels", timeout)) => floating_pixels(timeout, false),
        Ok(("floating-pixels-fullscreen", timeout)) => floating_pixels(timeout, true),
        Ok(("floating", timeout)) => floating(timeout),
        Ok(("fullscreen", timeout)) => fullscreen(timeout, false, false),
        Ok(("fullscreen-pixels", timeout)) => fullscreen(timeout, true, false),
        Ok(("fullscreen-pixels-partial", timeout)) => fullscreen(timeout, true, true),
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
