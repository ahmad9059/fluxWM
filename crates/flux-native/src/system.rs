// Uses pinned Smithay/calloop APIs and examples; see LICENSES/Smithay-MIT.txt.
//! Explicit Linux seat/device initialization. This module does not create DRM
//! surfaces or advertise a native desktop capability. Smithay may restore prior
//! DRM state during teardown; the explicit session probe requires a separate VT.
mod atomic;
mod discovery;
mod events;
mod target_test;
use crate::{Lifecycle, Snapshot, Ticket};
use serde::Serialize;
use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        drm::{DrmDevice, DrmDeviceFd, DrmNode, NodeType},
        egl::{EGLContext, EGLDisplay},
        input::InputEvent,
        libinput::{LibinputInputBackend, LibinputSessionInterface},
        renderer::{ImportDma, gles::GlesRenderer},
        session::{
            AsErrno, Event as SessionEvent, Session,
            libseat::{LibSeatSession, LibSeatSessionNotifier},
        },
        udev::{UdevBackend, UdevEvent},
    },
    reexports::{
        calloop::{Dispatcher, LoopHandle, RegistrationToken},
        drm::{Device as _, DriverCapability, control::Device as _},
        input::Libinput,
        rustix::fs::OFlags,
    },
    utils::DeviceFd,
};
use std::{
    any::Any,
    cell::{Cell, RefCell},
    collections::BTreeMap,
    os::{
        fd::{AsRawFd, OwnedFd},
        unix::fs::{FileTypeExt, MetadataExt},
    },
    path::{Path, PathBuf},
    rc::{Rc, Weak},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Stage {
    SeatOpen,
    SourceRegistration,
    UdevMonitor,
    InputSetup,
    InputResume,
    DeviceOpen,
    DeviceClose,
    SeatControl,
    DeviceMismatch,
    DeviceFlags,
    DeviceDuplicate,
    DrmInitialize,
    GbmInitialize,
    EglInitialize,
    RendererInitialize,
    DrmResources,
    OutputDiscovery,
    OutputTopology,
    DrmEvent,
    Lifecycle,
    Inactive,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct Failure {
    pub stage: Stage,
    pub errno: Option<i32>,
}
impl Failure {
    fn at(stage: Stage) -> Self {
        Self { stage, errno: None }
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "native operation failed: {:?}", self.stage)
    }
}
impl std::error::Error for Failure {}
impl AsErrno for Failure {
    fn as_errno(&self) -> Option<i32> {
        self.errno
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct GpuInfo {
    pub device: String,
    pub generation: String,
    pub session_epoch: String,
    pub connectors: usize,
    pub crtcs: usize,
    pub import_formats: usize,
    pub render_formats: usize,
    pub event_transport_healthy: bool,
    pub output_plan: Option<crate::output::Plan>,
    pub output_failure: Option<Failure>,
}
/// Input events are not stored in snapshots. Callers own their routing policy.
pub enum NativeEvent {
    SessionActive(bool),
    GpuReady(GpuInfo),
    GpuRemoved(u64),
    PageFlip {
        device: u64,
        generation: u64,
        session_epoch: u64,
        event: crate::drm_events::Flip,
        monotonic: bool,
    },
    Input(InputEvent<LibinputInputBackend>),
    Failure {
        device: Option<u64>,
        error: Failure,
    },
}

// Keep the broker's original descriptor until all private renderer/notifier
// duplicates are retired. Dropping an OwnedFd alone does not call Session::close.
struct SeatDevice<S: Session + Clone> {
    fd: Option<OwnedFd>,
    session: S,
    close_failures: Rc<Cell<u64>>,
    _owner: Option<Rc<dyn Any>>,
}
impl<S: Session + Clone> SeatDevice<S> {
    fn open(mut session: S, path: &Path, close_failures: Rc<Cell<u64>>) -> Result<Self, Failure> {
        if !session.is_active() {
            return Err(Failure::at(Stage::Inactive));
        }
        let fd = session
            .open(
                path,
                OFlags::RDWR | OFlags::CLOEXEC | OFlags::NONBLOCK | OFlags::NOCTTY,
            )
            .map_err(|e| Failure {
                stage: Stage::DeviceOpen,
                errno: e.as_errno(),
            })?;
        let device = Self {
            fd: Some(fd),
            session,
            close_failures,
            _owner: None,
        };
        enforce_flags(device.fd.as_ref().expect("owned descriptor")).map_err(|e| Failure {
            stage: Stage::DeviceFlags,
            errno: e.raw_os_error(),
        })?;
        Ok(device)
    }
    fn duplicate(&self) -> Result<OwnedFd, Failure> {
        self.fd
            .as_ref()
            .expect("owned descriptor")
            .try_clone()
            .map_err(|e| Failure {
                stage: Stage::DeviceDuplicate,
                errno: e.raw_os_error(),
            })
    }
}
fn enforce_flags(fd: &OwnedFd) -> std::io::Result<()> {
    let raw = fd.as_raw_fd();
    // SAFETY: the private live OwnedFd remains borrowed throughout these calls.
    unsafe {
        let flags = libc::fcntl(raw, libc::F_GETFD);
        if flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        let status = libc::fcntl(raw, libc::F_GETFL);
        if status < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if libc::fcntl(raw, libc::F_SETFD, flags | libc::FD_CLOEXEC) != 0
            || libc::fcntl(raw, libc::F_SETFL, status | libc::O_NONBLOCK) != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        let fd_flags = libc::fcntl(raw, libc::F_GETFD);
        let status_flags = libc::fcntl(raw, libc::F_GETFL);
        if fd_flags < 0 || status_flags < 0 {
            return Err(std::io::Error::last_os_error());
        }
        if fd_flags & libc::FD_CLOEXEC == 0 || status_flags & libc::O_NONBLOCK == 0 {
            return Err(std::io::Error::from_raw_os_error(libc::EIO));
        }
    }
    Ok(())
}
// libinput's session interface also routes through the flag-enforcing owner.
// Transfer its original FD to libinput; close_restricted returns it here.
#[derive(Clone)]
struct InputSession<S: Session + Clone> {
    session: S,
    close_failures: Rc<Cell<u64>>,
}
impl<S: Session + Clone> Session for InputSession<S> {
    type Error = Failure;
    fn open(&mut self, path: &Path, _flags: OFlags) -> Result<OwnedFd, Failure> {
        let mut device = SeatDevice::open(self.session.clone(), path, self.close_failures.clone())?;
        Ok(device.fd.take().expect("owned input descriptor"))
    }
    fn close(&mut self, fd: OwnedFd) -> Result<(), Failure> {
        self.session.close(fd).map_err(|e| {
            self.close_failures
                .set(self.close_failures.get().saturating_add(1));
            Failure {
                stage: Stage::DeviceClose,
                errno: e.as_errno(),
            }
        })
    }
    fn change_vt(&mut self, vt: i32) -> Result<(), Failure> {
        self.session.change_vt(vt).map_err(|e| Failure {
            stage: Stage::SeatControl,
            errno: e.as_errno(),
        })
    }
    fn is_active(&self) -> bool {
        self.session.is_active()
    }
    fn seat(&self) -> String {
        self.session.seat()
    }
}
/// Read-only udev inventory. This does not open a seat, DRM node or input device.
pub fn device_ids(seat: &str) -> Result<Vec<String>, Failure> {
    let udev = UdevBackend::new(seat).map_err(|e| Failure {
        stage: Stage::UdevMonitor,
        errno: e.raw_os_error(),
    })?;
    let mut ids: Vec<_> = udev.device_list().map(|(id, _)| id.to_string()).collect();
    ids.sort();
    Ok(ids)
}
impl<S: Session + Clone> Drop for SeatDevice<S> {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take()
            && self.session.close(fd).is_err()
        {
            self.close_failures
                .set(self.close_failures.get().saturating_add(1));
        }
    }
}
// Calloop can pin a removed dispatcher through the current callback. Its
// source field drops before its callback field in the pinned dependency.
// Hold the broker/session lease in that callback field until this point.
fn pin_lease<L, E, M, D, R>(
    lease: L,
    mut callback: impl FnMut(E, &mut M, &mut D) -> R,
) -> impl FnMut(E, &mut M, &mut D) -> R {
    move |event, metadata, data| {
        let _lease = &lease;
        callback(event, metadata, data)
    }
}
type WeakHandle<D> = Rc<dyn Fn() -> Option<LoopHandle<'static, D>>>;
struct Registration {
    remove: Box<dyn Fn()>,
}
impl Registration {
    fn new<D: 'static>(handle: WeakHandle<D>, token: RegistrationToken) -> Self {
        Self {
            remove: Box::new(move || {
                if let Some(handle) = handle() {
                    handle.remove(token);
                }
            }),
        }
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        (self.remove)();
    }
}
struct OwnedGpu<S: Session + Clone, R, G, D> {
    // This guard unregisters the notifier BEFORE graphics resources or broker FD.
    _source: Registration,
    _renderer: R,
    _gbm: G,
    drm: D,
    _seat_device: Rc<SeatDevice<S>>,
    ticket: Ticket,
    info: GpuInfo,
    _output_modes: BTreeMap<u32, Vec<smithay::reexports::drm::control::Mode>>,
}
type Gpu = OwnedGpu<LibSeatSession, GlesRenderer, GbmDevice<DrmDeviceFd>, DrmDevice>;
type Notify<D> = Rc<dyn Fn(NativeEvent, &mut D)>;
struct Kernel<D: 'static> {
    model: Lifecycle,
    session: LibSeatSession,
    paths: BTreeMap<u64, PathBuf>,
    gpus: BTreeMap<u64, Gpu>,
    input: Option<Libinput>,
    input_token: Option<Registration>,
    handle: WeakHandle<D>,
    notify: Notify<D>,
    close_failures: Rc<Cell<u64>>,
    seat_owner: Option<Rc<dyn Any>>,
}
/// Sources capture weak ownership. The independent session dispatcher outlives
/// input/GPU cleanup, including errors during partial initialization.
pub struct Driver<D: 'static> {
    kernel: Rc<RefCell<Kernel<D>>>,
    tokens: Vec<Registration>,
    _session_owner: Rc<Dispatcher<'static, LibSeatSessionNotifier, D>>,
}
impl<D: 'static> Driver<D> {
    pub fn new(
        handle: &LoopHandle<'static, D>,
        data: &mut D,
        notify: impl Fn(NativeEvent, &mut D) + 'static,
    ) -> Result<Self, Failure> {
        let (session, notifier) = LibSeatSession::new().map_err(|e| Failure {
            stage: Stage::SeatOpen,
            errno: e.as_errno(),
        })?;
        let notify: Notify<D> = Rc::new(notify);
        let weak_loop = handle.downgrade();
        let weak_handle: WeakHandle<D> = Rc::new(move || weak_loop.upgrade());
        let kernel = Rc::new(RefCell::new(Kernel {
            model: Lifecycle::default(),
            session: session.clone(),
            paths: BTreeMap::new(),
            gpus: BTreeMap::new(),
            input: None,
            input_token: None,
            handle: weak_handle.clone(),
            notify: notify.clone(),
            close_failures: Rc::new(Cell::new(0)),
            seat_owner: None,
        }));
        let weak = Rc::downgrade(&kernel);
        let owner = Rc::new(Dispatcher::new(notifier, move |event, _, data: &mut D| {
            let Some(kernel) = weak.upgrade() else { return };
            let events = match event {
                SessionEvent::PauseSession => kernel.borrow_mut().pause(),
                SessionEvent::ActivateSession => {
                    kernel.borrow_mut().activate(&Rc::downgrade(&kernel))
                }
            };
            let notify = kernel.borrow().notify.clone();
            dispatch_events(
                events,
                |event| notify(event, data),
                || !kernel.borrow().model.closed(),
            );
        }));
        kernel.borrow_mut().seat_owner = Some(owner.clone());
        let mut driver = Self {
            kernel,
            tokens: vec![],
            _session_owner: owner,
        };
        let token = handle
            .register_dispatcher((*driver._session_owner).clone())
            .map_err(|_| Failure::at(Stage::SourceRegistration))?;
        driver
            .tokens
            .push(Registration::new(weak_handle.clone(), token));
        let udev = UdevBackend::new(session.seat()).map_err(|e| Failure {
            stage: Stage::UdevMonitor,
            errno: e.raw_os_error(),
        })?;
        let initial: Vec<_> = udev
            .device_list()
            .map(|(id, path)| UdevEvent::Added {
                device_id: id,
                path: path.to_owned(),
            })
            .collect();
        let weak = Rc::downgrade(&driver.kernel);
        let token = handle
            .insert_source(udev, move |event, _, data| {
                let Some(kernel) = weak.upgrade() else { return };
                let events = kernel.borrow_mut().udev(event, &Rc::downgrade(&kernel));
                let notify = kernel.borrow().notify.clone();
                dispatch_events(
                    events,
                    |event| notify(event, data),
                    || !kernel.borrow().model.closed(),
                );
            })
            .map_err(|_| Failure::at(Stage::SourceRegistration))?;
        driver
            .tokens
            .push(Registration::new(weak_handle.clone(), token));
        for event in initial {
            let events = driver
                .kernel
                .borrow_mut()
                .udev(event, &Rc::downgrade(&driver.kernel));
            for event in events {
                notify(event, data);
            }
        }
        if session.is_active() {
            let events = driver
                .kernel
                .borrow_mut()
                .activate(&Rc::downgrade(&driver.kernel));
            for event in events {
                notify(event, data);
            }
        }
        Ok(driver)
    }
    /// Explicit manual allocation/render/atomic TEST_ONLY diagnostic. It does
    /// not create a surface or submit a frame. No resources escape this call.
    pub fn test_output_targets(&mut self, device: u64) -> crate::target::TestReport {
        let mut kernel = self.kernel.borrow_mut();
        let current = kernel.gpus.get(&device).is_some_and(|gpu| {
            kernel.model.is_ready(gpu.ticket) && gpu.info.event_transport_healthy
        });
        let plan = if current {
            kernel
                .gpus
                .get(&device)
                .and_then(|gpu| gpu.info.output_plan.clone())
        } else {
            None
        };
        let Some(plan) = plan else {
            return crate::target::TestReport {
                ok: false,
                planned_targets: 0,
                unassigned_connectors: 0,
                attempts: 0,
                nominal_bytes: "0".into(),
                tested_targets: vec![],
                failure: Some(crate::target::TestFailure::at(if current {
                    crate::target::TestStage::MissingPlan
                } else {
                    crate::target::TestStage::Inactive
                })),
                cleanup_failures: "0".into(),
            };
        };
        let gpu = kernel.gpus.get_mut(&device).expect("current GPU owned");
        target_test::test(
            &gpu.drm,
            &gpu._gbm,
            &mut gpu._renderer,
            &gpu._output_modes,
            &plan,
        )
    }
    pub fn snapshot(&self) -> Snapshot {
        self.kernel.borrow().model.snapshot()
    }
    pub fn gpu_info(&self) -> Vec<GpuInfo> {
        let kernel = self.kernel.borrow();
        kernel
            .gpus
            .values()
            .filter(|g| kernel.model.is_ready(g.ticket))
            .map(|g| g.info.clone())
            .collect()
    }
    pub fn close_failures(&self) -> u64 {
        self.kernel.borrow().close_failures.get()
    }
    pub fn stop(&mut self) {
        self.tokens.clear();
        self.kernel.borrow_mut().stop();
    }
}
impl<D: 'static> Drop for Driver<D> {
    fn drop(&mut self) {
        self.stop();
    }
}
// Context readiness survives an output-discovery failure. Consumers must gate
// output work on the explicit plan; the separate failure remains observable.
fn prepared_events(device: u64, info: GpuInfo) -> Vec<NativeEvent> {
    let failure = info.output_failure;
    let mut events = vec![NativeEvent::GpuReady(info)];
    if let Some(error) = failure {
        events.push(NativeEvent::Failure {
            device: Some(device),
            error,
        });
    }
    events
}
// Closed admission suppresses ready/input events, while terminal diagnostics
// and the final inactive transition remain observable, including after failure.
fn dispatch_events(
    events: impl IntoIterator<Item = NativeEvent>,
    mut notify: impl FnMut(NativeEvent),
    live: impl Fn() -> bool,
) {
    for event in events {
        if live()
            || matches!(
                event,
                NativeEvent::Failure { .. } | NativeEvent::SessionActive(false)
            )
        {
            notify(event);
        }
    }
}
// Pause active DRM even when a later initialization step returns early. The
// guard is disarmed only when ownership moves into the prepared GPU domain.
struct PauseOnDrop<T> {
    resource: Option<T>,
    pause: fn(&mut T),
}
impl<T> PauseOnDrop<T> {
    fn new(resource: T, pause: fn(&mut T)) -> Self {
        Self {
            resource: Some(resource),
            pause,
        }
    }
    fn disarm(mut self) -> T {
        self.resource.take().expect("pending resource owned")
    }
}
impl<T> std::ops::Deref for PauseOnDrop<T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.resource.as_ref().expect("pending resource owned")
    }
}
impl<T> Drop for PauseOnDrop<T> {
    fn drop(&mut self) {
        if let Some(resource) = &mut self.resource {
            (self.pause)(resource);
        }
    }
}
// Apply the pause before any fields retire. This suppresses Smithay's implicit
// active-device KMS restore on Changed/Removed/stop; no surfaces exist yet.
fn retire_paused<T>(resource: Option<T>, pause: impl FnOnce(&mut T)) {
    if let Some(mut resource) = resource {
        pause(&mut resource);
    }
}
impl<D: 'static> Kernel<D> {
    fn failure(device: Option<u64>, stage: Stage) -> NativeEvent {
        NativeEvent::Failure {
            device,
            error: Failure::at(stage),
        }
    }
    fn retire(&mut self, id: u64) {
        // Hotplug must not restore a connector snapshot captured before change.
        retire_paused(self.gpus.remove(&id), |gpu| {
            if gpu.drm.is_active() {
                gpu.drm.pause();
            }
        });
    }
    fn stop(&mut self) {
        self.model.stop();
        if let Some(input) = &mut self.input {
            input.suspend();
        }
        self.input_token.take();
        self.input.take();
        let devices: Vec<_> = self.gpus.keys().copied().collect();
        for id in devices {
            self.retire(id);
        }
        self.paths.clear();
    }
    fn pause(&mut self) -> Vec<NativeEvent> {
        match self.model.pause() {
            Ok(false) => vec![],
            Ok(true) => {
                if let Some(input) = &mut self.input {
                    input.suspend();
                }
                for gpu in self.gpus.values_mut() {
                    gpu.drm.pause();
                }
                vec![NativeEvent::SessionActive(false)]
            }
            Err(_) => {
                self.stop();
                vec![
                    Self::failure(None, Stage::Lifecycle),
                    NativeEvent::SessionActive(false),
                ]
            }
        }
    }
    fn activate(&mut self, weak: &Weak<RefCell<Self>>) -> Vec<NativeEvent> {
        match self.model.activate() {
            Ok(false) => return vec![],
            Ok(true) => (),
            Err(_) => {
                self.stop();
                return vec![
                    Self::failure(None, Stage::Lifecycle),
                    NativeEvent::SessionActive(false),
                ];
            }
        }
        let input_result = if let Some(input) = &mut self.input {
            input.resume().map_err(|_| Failure::at(Stage::InputResume))
        } else {
            self.init_input(weak)
        };
        if let Err(error) = input_result {
            self.stop();
            return vec![
                NativeEvent::Failure {
                    device: None,
                    error,
                },
                NativeEvent::SessionActive(false),
            ];
        }
        let mut events = vec![NativeEvent::SessionActive(true)];
        let ids: Vec<_> = self.paths.keys().copied().collect();
        for id in ids {
            if self.gpus.contains_key(&id) {
                self.retire(id);
                events.push(NativeEvent::GpuRemoved(id));
            }
            events.extend(self.prepare(id, weak));
        }
        events
    }
    fn init_input(&mut self, weak: &Weak<RefCell<Self>>) -> Result<(), Failure> {
        let mut input =
            Libinput::new_with_udev::<LibinputSessionInterface<InputSession<LibSeatSession>>>(
                InputSession {
                    session: self.session.clone(),
                    close_failures: self.close_failures.clone(),
                }
                .into(),
            );
        input
            .udev_assign_seat(&self.session.seat())
            .map_err(|_| Failure::at(Stage::InputSetup))?;
        let source = LibinputInputBackend::new(input.clone());
        let weak = weak.clone();
        let seat_owner = self.seat_owner.clone();
        let handle = (self.handle)().ok_or_else(|| Failure::at(Stage::SourceRegistration))?;
        let token = handle
            .insert_source(
                source,
                pin_lease(seat_owner, move |event, _, data| {
                    let Some(kernel) = weak.upgrade() else { return };
                    let notify = {
                        let k = kernel.borrow();
                        if !k.model.active() {
                            return;
                        };
                        k.notify.clone()
                    };
                    notify(NativeEvent::Input(event), data);
                }),
            )
            .map_err(|_| Failure::at(Stage::SourceRegistration))?;
        self.input = Some(input);
        self.input_token = Some(Registration::new(self.handle.clone(), token));
        Ok(())
    }
    fn udev(&mut self, event: UdevEvent, weak: &Weak<RefCell<Self>>) -> Vec<NativeEvent> {
        if self.model.closed() {
            return vec![];
        }
        let id = match &event {
            UdevEvent::Added { device_id, .. }
            | UdevEvent::Changed { device_id }
            | UdevEvent::Removed { device_id } => *device_id,
        };
        let result = match event {
            UdevEvent::Added { path, .. } => match self.model.discover(id) {
                Ok(false) => return vec![],
                Ok(true) => {
                    self.paths.insert(id, path);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            UdevEvent::Changed { .. } => {
                if !self.paths.contains_key(&id) {
                    return vec![];
                }
                self.model.changed(id)
            }
            UdevEvent::Removed { .. } => {
                self.retire(id);
                self.paths.remove(&id);
                return match self.model.remove(id) {
                    Ok(true) => vec![NativeEvent::GpuRemoved(id)],
                    Ok(false) => vec![],
                    Err(_) => vec![Self::failure(Some(id), Stage::Lifecycle)],
                };
            }
        };
        if result.is_err() {
            if self.model.closed() {
                self.stop();
                return vec![
                    Self::failure(Some(id), Stage::Lifecycle),
                    NativeEvent::SessionActive(false),
                ];
            }
            return vec![Self::failure(Some(id), Stage::Lifecycle)];
        }
        let mut events = vec![];
        if self.gpus.contains_key(&id) {
            self.retire(id);
            events.push(NativeEvent::GpuRemoved(id));
        }
        if self.model.active() {
            events.extend(self.prepare(id, weak));
        }
        events
    }
    fn prepare(&mut self, id: u64, weak: &Weak<RefCell<Self>>) -> Vec<NativeEvent> {
        let ticket = match self.model.begin(id) {
            Ok(ticket) => ticket,
            Err(_) => return vec![Self::failure(Some(id), Stage::Lifecycle)],
        };
        let result = self.create_gpu(ticket, weak);
        let success = result.is_ok();
        if self.model.complete(ticket, success).is_err() {
            retire_paused(result.ok(), |gpu| gpu.drm.pause());
            return vec![Self::failure(Some(id), Stage::Lifecycle)];
        }
        match result {
            Ok(gpu) => {
                let info = gpu.info.clone();
                self.gpus.insert(id, gpu);
                prepared_events(id, info)
            }
            Err(error) => vec![NativeEvent::Failure {
                device: Some(id),
                error,
            }],
        }
    }
    fn create_gpu(&mut self, ticket: Ticket, weak: &Weak<RefCell<Self>>) -> Result<Gpu, Failure> {
        let path = self
            .paths
            .get(&ticket.device)
            .ok_or_else(|| Failure::at(Stage::DeviceMismatch))?;
        let meta = std::fs::symlink_metadata(path).map_err(|e| Failure {
            stage: Stage::DeviceMismatch,
            errno: e.raw_os_error(),
        })?;
        if !meta.file_type().is_char_device() || meta.rdev() != ticket.device {
            return Err(Failure::at(Stage::DeviceMismatch));
        }
        let mut seat_device =
            SeatDevice::open(self.session.clone(), path, self.close_failures.clone())?;
        seat_device._owner = self.seat_owner.clone();
        let seat_device = Rc::new(seat_device);
        let raw = seat_device.fd.as_ref().expect("owned descriptor");
        let node = DrmNode::from_file(raw).map_err(|_| Failure::at(Stage::DeviceMismatch))?;
        if node.dev_id() != ticket.device || node.ty() != NodeType::Primary {
            return Err(Failure::at(Stage::DeviceMismatch));
        }
        let fd = DrmDeviceFd::new(DeviceFd::from(seat_device.duplicate()?));
        // false preserves current connector state; this foundation creates no surfaces.
        let (drm, smithay_notifier) =
            DrmDevice::new(fd.clone(), false).map_err(|_| Failure::at(Stage::DrmInitialize))?;
        let drm = PauseOnDrop::new(drm, DrmDevice::pause);
        // drm-rs/Smithay discard page-flip user_data. This private reader is the
        // only consumer of this open-file-description's event stream.
        drop(smithay_notifier);
        if drm
            .get_driver_capability(DriverCapability::CRTCInVBlankEvent)
            .ok()
            != Some(1)
        {
            return Err(Failure::at(Stage::DrmEvent));
        }
        let monotonic = drm
            .get_driver_capability(DriverCapability::MonotonicTimestamp)
            .ok()
            == Some(1);
        let notifier = events::Source::new(seat_device.duplicate()?).map_err(|e| Failure {
            stage: Stage::DrmEvent,
            errno: e.raw_os_error(),
        })?;
        let gbm = GbmDevice::new(fd).map_err(|_| Failure::at(Stage::GbmInitialize))?;
        let display = unsafe { EGLDisplay::new(gbm.clone()) }
            .map_err(|_| Failure::at(Stage::EglInitialize))?;
        let context = EGLContext::new(&display).map_err(|_| Failure::at(Stage::EglInitialize))?;
        let renderer = unsafe { GlesRenderer::new(context) }
            .map_err(|_| Failure::at(Stage::RendererInitialize))?;
        let resources = drm
            .resource_handles()
            .map_err(|_| Failure::at(Stage::DrmResources))?;
        let outputs = discovery::prepare(
            &drm,
            renderer
                .egl_context()
                .dmabuf_render_formats()
                .iter()
                .take(crate::output::MAX_FORMATS + 1)
                .map(|format| crate::output::Format {
                    code: format.code as u32,
                    modifier: format.modifier.into(),
                })
                .collect(),
        );
        let info = GpuInfo {
            device: ticket.device.to_string(),
            generation: ticket.generation.to_string(),
            session_epoch: ticket.session_epoch.to_string(),
            connectors: resources.connectors().len(),
            crtcs: resources.crtcs().len(),
            import_formats: renderer.dmabuf_formats().iter().count(),
            event_transport_healthy: true,
            output_plan: outputs.plan,
            output_failure: outputs.failure,
            render_formats: renderer
                .egl_context()
                .dmabuf_render_formats()
                .iter()
                .count(),
        };
        let handle = (self.handle)().ok_or_else(|| Failure::at(Stage::SourceRegistration))?;
        let weak = weak.clone();
        let token = handle
            .insert_source(
                notifier,
                pin_lease(seat_device.clone(), move |event, _, data| {
                    let Some(kernel) = weak.upgrade() else { return };
                    let notify = {
                        let k = kernel.borrow();
                        if !k.model.is_ready(ticket) {
                            return;
                        };
                        k.notify.clone()
                    };
                    let event = match event {
                        Ok(event) => NativeEvent::PageFlip {
                            device: ticket.device,
                            generation: ticket.generation,
                            session_epoch: ticket.session_epoch,
                            event,
                            monotonic,
                        },
                        Err(error) => {
                            if let Some(gpu) = kernel.borrow_mut().gpus.get_mut(&ticket.device) {
                                gpu.info.event_transport_healthy = false;
                            }
                            NativeEvent::Failure {
                                device: Some(ticket.device),
                                error: Failure {
                                    stage: Stage::DrmEvent,
                                    errno: match error {
                                        events::Failure::Read(errno) => errno,
                                        _ => None,
                                    },
                                },
                            }
                        }
                    };
                    notify(event, data);
                }),
            )
            .map_err(|_| Failure::at(Stage::SourceRegistration))?;
        Ok(OwnedGpu {
            _source: Registration::new(self.handle.clone(), token),
            _renderer: renderer,
            _gbm: gbm,
            drm: drm.disarm(),
            _seat_device: seat_device,
            ticket,
            info,
            _output_modes: outputs.modes,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::calloop::{
        EventLoop, EventSource, Poll, PostAction, Readiness, Token, TokenFactory,
        ping::{PingSource, make_ping},
    };
    use std::time::Duration;
    type Log = Rc<RefCell<Vec<&'static str>>>;
    #[derive(Clone)]
    struct MockSession {
        log: Log,
        active: bool,
        fail_open: bool,
        fail_close: bool,
        path_only: bool,
    }
    impl Session for MockSession {
        type Error = ();
        fn open(&mut self, _: &Path, _: OFlags) -> Result<OwnedFd, ()> {
            self.log.borrow_mut().push("open");
            if self.fail_open {
                return Err(());
            }
            if self.path_only {
                use std::os::unix::fs::OpenOptionsExt;
                return std::fs::OpenOptions::new()
                    .read(true)
                    .custom_flags(libc::O_PATH)
                    .open("/dev/null")
                    .map(Into::into)
                    .map_err(|_| ());
            }
            let fd: OwnedFd = std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open("/dev/null")
                .map_err(|_| ())?
                .into();
            // Deliberately emulate a broker adapter that ignores requested flags.
            unsafe {
                let raw = fd.as_raw_fd();
                assert_eq!(libc::fcntl(raw, libc::F_SETFD, 0), 0);
                let status = libc::fcntl(raw, libc::F_GETFL);
                assert_eq!(
                    libc::fcntl(raw, libc::F_SETFL, status & !libc::O_NONBLOCK),
                    0
                );
            }
            Ok(fd)
        }
        fn close(&mut self, _fd: OwnedFd) -> Result<(), ()> {
            self.log.borrow_mut().push("close");
            if self.fail_close { Err(()) } else { Ok(()) }
        }
        fn change_vt(&mut self, _: i32) -> Result<(), ()> {
            Err(())
        }
        fn is_active(&self) -> bool {
            self.active
        }
        fn seat(&self) -> String {
            "mock".into()
        }
    }
    fn session(log: Log) -> MockSession {
        MockSession {
            log,
            active: true,
            fail_open: false,
            fail_close: false,
            path_only: false,
        }
    }
    #[test]
    fn broker_flags_are_enforced_on_original_and_renderer_duplicate() {
        let log = Log::default();
        let failures = Rc::new(Cell::new(0));
        let d =
            SeatDevice::open(session(log.clone()), Path::new("fixture"), failures.clone()).unwrap();
        let clone = d.duplicate().unwrap();
        for fd in [d.fd.as_ref().unwrap(), &clone] {
            unsafe {
                assert_ne!(
                    libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) & libc::FD_CLOEXEC,
                    0
                );
                assert_ne!(
                    libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) & libc::O_NONBLOCK,
                    0
                );
            }
        }
        drop(clone);
        drop(d);
        assert_eq!(*log.borrow(), ["open", "close"]);
        assert_eq!(failures.get(), 0);
    }
    #[test]
    fn failed_post_open_setup_still_closes_broker_device_once() {
        let log = Log::default();
        let mut s = session(log.clone());
        s.path_only = true;
        let result = SeatDevice::open(s, Path::new("fixture"), Rc::new(Cell::new(0)));
        let Err(error) = result else {
            panic!("O_PATH flag enforcement failure accepted")
        };
        assert_eq!(error.stage, Stage::DeviceFlags);
        assert_eq!(error.errno, Some(libc::EBADF));
        assert_eq!(*log.borrow(), ["open", "close"]);
    }
    #[test]
    fn inactive_or_failed_open_never_produces_an_owned_device() {
        let log = Log::default();
        let mut s = session(log.clone());
        s.active = false;
        let Err(error) = SeatDevice::open(s, Path::new("fixture"), Rc::new(Cell::new(0))) else {
            panic!("inactive open")
        };
        assert_eq!(error.stage, Stage::Inactive);
        assert!(log.borrow().is_empty());
        let mut s = session(log.clone());
        s.fail_open = true;
        assert!(SeatDevice::open(s, Path::new("fixture"), Rc::new(Cell::new(0))).is_err());
        assert_eq!(*log.borrow(), ["open"]);
    }
    #[test]
    fn failed_broker_close_is_counted_and_not_repeated() {
        let log = Log::default();
        let mut s = session(log.clone());
        s.fail_close = true;
        let failures = Rc::new(Cell::new(0));
        drop(SeatDevice::open(s, Path::new("fixture"), failures.clone()).unwrap());
        assert_eq!(failures.get(), 1);
        assert_eq!(*log.borrow(), ["open", "close"]);
    }
    #[test]
    fn registration_does_not_retain_event_loop_and_survives_loop_first_drop() {
        let event_loop = EventLoop::<()>::try_new().unwrap();
        let weak = event_loop.handle().downgrade();
        let upgrade: WeakHandle<()> = Rc::new(move || weak.upgrade());
        let (_, source) = make_ping().unwrap();
        let token = event_loop
            .handle()
            .insert_source(source, |_, _, _| {})
            .unwrap();
        let registration = Registration::new(upgrade.clone(), token);
        drop(event_loop);
        assert!(upgrade().is_none());
        drop(registration);
    }
    #[test]
    fn input_adapter_enforces_flags_transfers_ownership_and_accounts_close_failure() {
        let log = Log::default();
        let mut s = session(log.clone());
        s.fail_close = true;
        let failures = Rc::new(Cell::new(0));
        let mut input = InputSession {
            session: s,
            close_failures: failures.clone(),
        };
        let fd = input.open(Path::new("fixture"), OFlags::RDWR).unwrap();
        assert_eq!(
            *log.borrow(),
            ["open"],
            "temporary owner must not close the transferred FD"
        );
        unsafe {
            assert_ne!(
                libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) & libc::FD_CLOEXEC,
                0
            );
            assert_ne!(
                libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) & libc::O_NONBLOCK,
                0
            );
        }
        assert_eq!(input.close(fd).unwrap_err().stage, Stage::DeviceClose);
        assert_eq!(failures.get(), 1);
        assert_eq!(*log.borrow(), ["open", "close"]);
    }
    #[test]
    fn input_adapter_flag_failure_closes_without_transferring_or_retrying() {
        let log = Log::default();
        let mut s = session(log.clone());
        s.path_only = true;
        let mut input = InputSession {
            session: s,
            close_failures: Rc::new(Cell::new(0)),
        };
        assert_eq!(
            input
                .open(Path::new("fixture"), OFlags::RDWR)
                .unwrap_err()
                .stage,
            Stage::DeviceFlags
        );
        assert_eq!(*log.borrow(), ["open", "close"]);
    }
    struct Trace {
        log: Log,
        label: &'static str,
    }
    impl Drop for Trace {
        fn drop(&mut self) {
            self.log.borrow_mut().push(self.label);
        }
    }
    struct TraceSource {
        source: PingSource,
        fd: Option<OwnedFd>,
        log: Log,
    }
    impl Drop for TraceSource {
        fn drop(&mut self) {
            self.fd.take();
            self.log.borrow_mut().push("notifier");
        }
    }
    impl EventSource for TraceSource {
        type Event = ();
        type Metadata = ();
        type Ret = ();
        type Error = <PingSource as EventSource>::Error;
        fn process_events<F>(
            &mut self,
            r: Readiness,
            t: Token,
            f: F,
        ) -> Result<PostAction, Self::Error>
        where
            F: FnMut((), &mut ()),
        {
            self.source.process_events(r, t, f)
        }
        fn register(
            &mut self,
            p: &mut Poll,
            f: &mut TokenFactory,
        ) -> smithay::reexports::calloop::Result<()> {
            self.source.register(p, f)
        }
        fn reregister(
            &mut self,
            p: &mut Poll,
            f: &mut TokenFactory,
        ) -> smithay::reexports::calloop::Result<()> {
            self.source.reregister(p, f)
        }
        fn unregister(&mut self, p: &mut Poll) -> smithay::reexports::calloop::Result<()> {
            self.source.unregister(p)
        }
    }
    #[test]
    fn output_failure_reports_context_readiness_and_explicit_missing_plan() {
        let failure = Failure::at(Stage::OutputTopology);
        let info = GpuInfo {
            device: "42".into(),
            generation: "1".into(),
            session_epoch: "1".into(),
            connectors: 1,
            crtcs: 1,
            import_formats: 1,
            render_formats: 257,
            event_transport_healthy: true,
            output_plan: None,
            output_failure: Some(failure),
        };
        let events = prepared_events(42, info.clone());
        assert!(
            matches!(&events[0], NativeEvent::GpuReady(info) if info.output_plan.is_none() && info.output_failure == Some(failure))
        );
        assert!(
            matches!(&events[1], NativeEvent::Failure {device:Some(42), error} if *error == failure)
        );
        assert_eq!(events.len(), 2);
        let events = prepared_events(
            42,
            GpuInfo {
                output_plan: Some(crate::output::Plan::default()),
                output_failure: None,
                ..info
            },
        );
        assert_eq!(events.len(), 1);
    }
    #[test]
    fn notification_shutdown_suppresses_ready_but_preserves_terminal_events() {
        let live = Cell::new(true);
        let mut seen = vec![];
        let events = [
            NativeEvent::SessionActive(true),
            NativeEvent::GpuRemoved(1),
            NativeEvent::Failure {
                device: None,
                error: Failure::at(Stage::Lifecycle),
            },
            NativeEvent::SessionActive(false),
        ];
        dispatch_events(
            events,
            |event| {
                seen.push(match event {
                    NativeEvent::SessionActive(true) => "active",
                    NativeEvent::SessionActive(false) => "inactive",
                    NativeEvent::Failure { .. } => "failure",
                    _ => panic!("nonterminal event delivered after shutdown"),
                });
                live.set(false);
            },
            || live.get(),
        );
        assert_eq!(seen, ["active", "failure", "inactive"]);
        let mut terminal = vec![];
        dispatch_events(
            [
                NativeEvent::Failure {
                    device: None,
                    error: Failure::at(Stage::InputSetup),
                },
                NativeEvent::SessionActive(false),
            ],
            |event| terminal.push(matches!(event, NativeEvent::SessionActive(false))),
            || false,
        );
        assert_eq!(terminal, [false, true]);
    }
    #[test]
    fn partial_initialization_guard_pauses_on_error_and_disarms_on_handoff() {
        struct Device(Rc<Cell<bool>>);
        impl Drop for Device {
            fn drop(&mut self) {
                assert!(self.0.get(), "unpaused KMS restore");
            }
        }
        fn prepare(paused: Rc<Cell<bool>>) -> Result<(), ()> {
            let _guard = PauseOnDrop::new(Device(paused), |device| device.0.set(true));
            Err(())
        }
        let paused = Rc::new(Cell::new(false));
        assert!(prepare(paused.clone()).is_err());
        assert!(paused.get());
        paused.set(false);
        let device = PauseOnDrop::new(Device(paused.clone()), |device| device.0.set(true)).disarm();
        assert!(!paused.get(), "handoff must preserve active resource");
        device.0.set(true);
    }
    #[test]
    fn retirement_pauses_before_drop_and_ignores_absent_resources() {
        struct Device(Rc<Cell<bool>>);
        impl Drop for Device {
            fn drop(&mut self) {
                assert!(self.0.get(), "active drop would restore stale KMS state");
            }
        }
        let paused = Rc::new(Cell::new(false));
        retire_paused(Some(Device(paused.clone())), |device| device.0.set(true));
        assert!(paused.get());
        retire_paused(None::<Device>, |_| panic!("no device to pause"));
    }
    type TestGpu = OwnedGpu<MockSession, Trace, Trace, Trace>;
    #[derive(Default)]
    struct TestState {
        gpu: Option<TestGpu>,
    }
    #[test]
    fn reentrant_removal_keeps_broker_and_session_until_dispatcher_fd_retires() {
        let log = Log::default();
        let mut event_loop = EventLoop::<TestState>::try_new().unwrap();
        let mut d = SeatDevice::open(
            session(log.clone()),
            Path::new("fixture"),
            Rc::new(Cell::new(0)),
        )
        .unwrap();
        d._owner = Some(Rc::new(Trace {
            log: log.clone(),
            label: "seat-owner",
        }));
        let lease = Rc::new(d);
        let (ping, source) = make_ping().unwrap();
        let source = TraceSource {
            source,
            fd: Some(lease.duplicate().unwrap()),
            log: log.clone(),
        };
        let callback_log = log.clone();
        let token = event_loop
            .handle()
            .insert_source(
                source,
                pin_lease(lease.clone(), move |(), _, state: &mut TestState| {
                    state.gpu.take();
                    callback_log.borrow_mut().push("callback-end");
                }),
            )
            .unwrap();
        let weak = event_loop.handle().downgrade();
        let registration = Registration::new(Rc::new(move || weak.upgrade()), token);
        let t = Ticket {
            device: 1,
            generation: 1,
            session_epoch: 1,
        };
        let trace = |label| Trace {
            log: log.clone(),
            label,
        };
        let mut state = TestState {
            gpu: Some(OwnedGpu {
                _source: registration,
                _renderer: trace("renderer"),
                _gbm: trace("gbm"),
                drm: trace("drm"),
                _seat_device: lease,
                ticket: t,
                _output_modes: BTreeMap::new(),
                info: GpuInfo {
                    device: "1".into(),
                    generation: "1".into(),
                    session_epoch: "1".into(),
                    connectors: 0,
                    crtcs: 0,
                    import_formats: 0,
                    render_formats: 0,
                    event_transport_healthy: true,
                    output_plan: Some(crate::output::Plan::default()),
                    output_failure: None,
                },
            }),
        };
        ping.ping();
        event_loop
            .dispatch(Duration::from_millis(100), &mut state)
            .unwrap();
        assert!(state.gpu.is_none());
        assert_eq!(
            *log.borrow(),
            [
                "open",
                "renderer",
                "gbm",
                "drm",
                "callback-end",
                "notifier",
                "close",
                "seat-owner"
            ]
        );
    }
}
