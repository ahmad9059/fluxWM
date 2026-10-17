//! Optional calloop ownership/registration adapter. No compositor policy lives here.
use crate::{Dispatch, DrainState, Endpoint, IoInterest, Request, Response, Server, ServerStats};
use calloop::{
    Interest, LoopHandle, Mode, PostAction, RegistrationToken,
    generic::Generic,
    ping::{Ping, make_ping},
    timer::{TimeoutAction, Timer},
};
use std::{
    cell::Cell,
    collections::HashMap,
    io,
    os::fd::{FromRawFd, OwnedFd},
    path::Path,
    rc::Rc,
    time::Instant,
};

struct Registration {
    token: RegistrationToken,
    readable: bool,
    writable: bool,
}
struct Deadline {
    instant: Instant,
    token: RegistrationToken,
}

/// Drives `Server` with readiness, coalesced buffered-work pings and exact timers.
///
/// Source callbacks only invoke `wake`; that callback must mark compositor work
/// pending. Call `poll` after event-loop dispatch, never reentrantly inside `wake`.
/// Registered Generic sources own duplicates of the server's socket descriptors,
/// so removing a server connection cannot invalidate a registered borrowed fd.
/// Only a WeakLoopHandle is retained: dropping the loop cannot form an Rc cycle.
pub struct EventLoopDriver<D: 'static> {
    // Drop unregisters every source before this server closes/unlinks its socket.
    server: Server,
    // calloop 0.14.5 returns WeakLoopHandle but does not re-export its type.
    // Erase only the upgrade closure; its capture is still the weak handle.
    handle: Box<dyn Fn() -> Option<LoopHandle<'static, D>>>,
    registrations: HashMap<Endpoint, Registration>,
    ping: Ping,
    ping_registration: Option<RegistrationToken>,
    ping_pending: Rc<Cell<bool>>,
    deadline: Option<Deadline>,
    deadline_fired: Rc<Cell<bool>>,
    wake: fn(&mut D),
}

impl<D: 'static> EventLoopDriver<D> {
    pub fn bind(
        handle: &LoopHandle<'static, D>,
        runtime_dir: &Path,
        instance: &str,
        wake: fn(&mut D),
    ) -> io::Result<Self> {
        let server = Server::bind(runtime_dir, instance)?;
        let (ping, source) = make_ping()?;
        let pending = Rc::new(Cell::new(false));
        let weak_handle = handle.downgrade();
        let mut driver = Self {
            server,
            handle: Box::new(move || weak_handle.upgrade()),
            registrations: HashMap::new(),
            ping,
            ping_registration: None,
            ping_pending: pending.clone(),
            deadline: None,
            deadline_fired: Rc::new(Cell::new(false)),
            wake,
        };
        driver.ping_registration = Some(
            handle
                .insert_source(source, move |(), &mut (), state| {
                    if pending.replace(false) {
                        wake(state);
                    }
                })
                .map_err(|error| io::Error::other(error.error.to_string()))?,
        );
        driver.reconcile(handle, Instant::now())?;
        Ok(driver)
    }

    /// Invoke once after the compositor's wake flag is observed, or when other
    /// core work requires IPC servicing. Handler must report exact current sequence.
    /// An error is an adapter/registration failure; drop the driver to stop IPC
    /// and remove every installed source before recovering or ending the session.
    pub fn poll(
        &mut self,
        now: Instant,
        sequence: u64,
        handler: impl FnMut(Request) -> Response,
    ) -> io::Result<()> {
        let handle = (self.handle)().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "IPC event loop was dropped")
        })?;
        self.server.set_state_sequence(sequence);
        self.server.poll(now, handler);
        self.reconcile(&handle, now)
    }

    pub fn poll_controlled(
        &mut self,
        now: Instant,
        sequence: u64,
        handler: impl FnMut(Request) -> Dispatch,
    ) -> io::Result<()> {
        let handle = (self.handle)().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "IPC event loop was dropped")
        })?;
        self.server.set_state_sequence(sequence);
        self.server.poll_controlled(now, handler);
        self.reconcile(&handle, now)
    }
    pub fn begin_drain(&mut self, deadline: Instant) -> io::Result<()> {
        let handle = (self.handle)().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "IPC event loop was dropped")
        })?;
        self.server.begin_drain(deadline);
        self.reconcile(&handle, Instant::now())
    }
    pub fn drain_state(&self) -> DrainState {
        self.server.drain_state()
    }

    pub fn socket_path(&self) -> &Path {
        self.server.socket_path()
    }

    pub fn stats(&self) -> ServerStats {
        self.server.stats()
    }

    pub fn connection_count(&self) -> usize {
        self.server.connection_count()
    }

    fn reconcile(&mut self, handle: &LoopHandle<'static, D>, now: Instant) -> io::Result<()> {
        let readiness = self.server.readiness(now);
        // Remove obsolete or changed registrations before adding any replacement.
        // Endpoint identity, not fd number, distinguishes reused client sockets.
        self.registrations.retain(|endpoint, registered| {
            let unchanged = readiness.interests.iter().any(|interest| {
                interest.endpoint == *endpoint
                    && interest.readable == registered.readable
                    && interest.writable == registered.writable
            });
            if !unchanged {
                handle.remove(registered.token);
            }
            unchanged
        });
        for interest in readiness.interests {
            if !self.registrations.contains_key(&interest.endpoint) {
                let registration = register_socket(handle, interest, self.wake)?;
                self.registrations.insert(interest.endpoint, registration);
            }
        }
        if readiness.needs_dispatch {
            if !self.ping_pending.replace(true) {
                self.ping.ping();
            }
        } else {
            // A previously queued ping may still arrive; drain it without a wake
            // when another caller already consumed all buffered work.
            self.ping_pending.set(false);
        }
        let timer_unchanged = !self.deadline_fired.get()
            && self.deadline.as_ref().map(|timer| timer.instant) == readiness.next_deadline;
        if !timer_unchanged {
            if let Some(previous) = self.deadline.take() {
                handle.remove(previous.token);
            }
            self.deadline_fired.set(false);
            if let Some(instant) = readiness.next_deadline {
                let fired = self.deadline_fired.clone();
                let wake = self.wake;
                let token = handle
                    .insert_source(Timer::from_deadline(instant), move |_, &mut (), state| {
                        fired.set(true);
                        wake(state);
                        TimeoutAction::Drop
                    })
                    .map_err(|error| io::Error::other(error.error.to_string()))?;
                self.deadline = Some(Deadline { instant, token });
            }
        }
        Ok(())
    }
}

fn register_socket<D: 'static>(
    handle: &LoopHandle<'static, D>,
    interest: IoInterest,
    wake: fn(&mut D),
) -> io::Result<Registration> {
    // Server owns this valid fd throughout reconciliation. CLOEXEC is set
    // atomically; the duplicate independently owns its kernel socket reference.
    let raw = unsafe { libc::fcntl(interest.fd, libc::F_DUPFD_CLOEXEC, 0) };
    if raw < 0 {
        return Err(io::Error::last_os_error());
    }
    let owned = unsafe { OwnedFd::from_raw_fd(raw) };
    let interest_flags = Interest {
        readable: interest.readable,
        writable: interest.writable,
    };
    let token = handle
        .insert_source(
            Generic::new(owned, interest_flags, Mode::Level),
            move |_, _, state| {
                wake(state);
                Ok(PostAction::Continue)
            },
        )
        .map_err(|error| io::Error::other(error.error.to_string()))?;
    Ok(Registration {
        token,
        readable: interest.readable,
        writable: interest.writable,
    })
}

impl<D: 'static> Drop for EventLoopDriver<D> {
    fn drop(&mut self) {
        if let Some(handle) = (self.handle)() {
            for (_, registration) in self.registrations.drain() {
                handle.remove(registration.token);
            }
            if let Some(timer) = self.deadline.take() {
                handle.remove(timer.token);
            }
            if let Some(ping) = self.ping_registration.take() {
                handle.remove(ping);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Method, decode, encode};
    use calloop::EventLoop;
    use std::{
        fs,
        io::{Read, Write},
        net::Shutdown,
        os::unix::{fs::DirBuilderExt, net::UnixStream},
        path::PathBuf,
        time::Duration,
    };

    struct Temp(PathBuf);
    impl Temp {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let id = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path =
                std::env::temp_dir().join(format!("flux-calloop-{}-{id}", std::process::id()));
            fs::DirBuilder::new().mode(0o700).create(&path).unwrap();
            Self(path)
        }
    }
    impl Drop for Temp {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    #[derive(Default)]
    struct State {
        pending: bool,
        wakes: u64,
        handled: usize,
    }
    fn wake(state: &mut State) {
        state.pending = true;
        state.wakes += 1;
    }
    struct Fixture {
        event_loop: EventLoop<'static, State>,
        driver: EventLoopDriver<State>,
        state: State,
        _temp: Temp,
    }
    impl Fixture {
        fn new() -> Self {
            let temp = Temp::new();
            let event_loop = EventLoop::try_new().unwrap();
            let driver =
                EventLoopDriver::bind(&event_loop.handle(), &temp.0, "test", wake).unwrap();
            Self {
                event_loop,
                driver,
                state: State::default(),
                _temp: temp,
            }
        }
        fn client(&self) -> UnixStream {
            UnixStream::connect(self.driver.socket_path()).unwrap()
        }
        fn dispatch(&mut self, timeout: Duration) {
            self.event_loop.dispatch(timeout, &mut self.state).unwrap();
            if std::mem::take(&mut self.state.pending) {
                self.driver
                    .poll(Instant::now(), 0, |request| {
                        self.state.handled += 1;
                        Response::success(request.id, 0, serde_json::Value::Null)
                    })
                    .unwrap();
            }
        }
    }
    fn read_response(client: &mut UnixStream) -> Response {
        client
            .set_read_timeout(Some(Duration::from_secs(1)))
            .unwrap();
        let mut prefix = [0; 4];
        client.read_exact(&mut prefix).unwrap();
        let mut body = vec![0; u32::from_be_bytes(prefix) as usize];
        client.read_exact(&mut body).unwrap();
        decode(&body).unwrap()
    }

    #[test]
    fn buffered_work_continues_through_ping_without_more_client_bytes() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        let mut frames = Vec::new();
        for index in 0..40 {
            frames.extend(encode(&Request::new(index.to_string(), Method::Version)).unwrap());
        }
        client.write_all(&frames).unwrap();
        fixture.dispatch(Duration::from_millis(100));
        assert_eq!(fixture.state.handled, 16);
        assert!(fixture.driver.ping_pending.get());
        for _ in 0..5 {
            if fixture.state.handled == 40 {
                break;
            }
            fixture.dispatch(Duration::from_millis(100));
        }
        assert_eq!(fixture.state.handled, 40);
        for index in 0..40 {
            assert_eq!(read_response(&mut client).id, index.to_string());
        }
        let wakes = fixture.state.wakes;
        fixture.dispatch(Duration::from_millis(25));
        assert_eq!(fixture.state.wakes, wakes);
    }

    #[test]
    fn eof_flushes_response_and_removes_duplicate_owned_fd() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        client
            .write_all(&encode(&Request::new("half", Method::Version)).unwrap())
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        for _ in 0..4 {
            fixture.dispatch(Duration::from_millis(25));
            if fixture.driver.connection_count() == 0 {
                break;
            }
        }
        assert_eq!(fixture.state.handled, 1);
        assert_eq!(fixture.driver.connection_count(), 0);
        assert_eq!(read_response(&mut client).id, "half");
        let mut byte = [0];
        assert_eq!(client.read(&mut byte).unwrap(), 0);
        assert_eq!(fixture.driver.registrations.len(), 1);
    }

    #[test]
    fn exact_partial_deadline_wakes_without_client_or_periodic_polling() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        client.write_all(&[0, 0]).unwrap();
        fixture.dispatch(Duration::from_millis(100));
        assert_eq!(fixture.driver.connection_count(), 1);
        // Keep the real five-second policy but position its start 40ms before expiry.
        // This isolates the actual calloop timer path without a five-second sleep.
        let now = Instant::now();
        let expected = now + Duration::from_millis(40);
        fixture.driver.server.connections[0].partial_since = Some(expected - crate::DEADLINE);
        let handle = fixture.event_loop.handle();
        fixture.driver.reconcile(&handle, now).unwrap();
        drop(handle);
        assert_eq!(fixture.driver.deadline.as_ref().unwrap().instant, expected);
        fixture.dispatch(Duration::from_millis(200));
        assert_eq!(fixture.driver.connection_count(), 0);
        assert_eq!(fixture.driver.stats().timed_out, 1);
        assert!(fixture.driver.deadline.is_none());
        let wakes = fixture.state.wakes;
        fixture.dispatch(Duration::from_millis(25));
        assert_eq!(fixture.state.wakes, wakes);
    }

    #[test]
    fn capacity_backoff_removes_listener_until_exact_retry_timer() {
        let mut fixture = Fixture::new();
        let mut clients = Vec::new();
        for _ in 0..crate::MAX_CONNECTIONS + 1 {
            clients.push(fixture.client());
        }
        fixture.dispatch(Duration::from_millis(100));
        fixture.dispatch(Duration::from_millis(100));
        assert_eq!(fixture.driver.connection_count(), crate::MAX_CONNECTIONS);
        assert_eq!(fixture.driver.stats().rejected_capacity, 1);
        assert!(
            !fixture
                .driver
                .registrations
                .contains_key(&Endpoint::Listener)
        );
        assert!(fixture.driver.registrations.len() <= crate::MAX_CONNECTIONS + 1);
        let retry = fixture.driver.server.accept_retry_at.unwrap();
        assert_eq!(fixture.driver.deadline.as_ref().unwrap().instant, retry);
        fixture.dispatch(Duration::from_millis(200));
        assert!(
            fixture
                .driver
                .registrations
                .contains_key(&Endpoint::Listener)
        );
        assert!(fixture.driver.registrations.len() <= crate::MAX_CONNECTIONS + 1);
    }

    #[test]
    fn idle_adapter_has_no_timer_or_repeated_wakes() {
        let mut fixture = Fixture::new();
        assert!(fixture.driver.deadline.is_none());
        assert!(!fixture.driver.ping_pending.get());
        let started = Instant::now();
        for _ in 0..3 {
            fixture.dispatch(Duration::from_millis(15));
        }
        assert!(started.elapsed() >= Duration::from_millis(35));
        assert_eq!(fixture.state.wakes, 0);
        assert!(fixture.driver.deadline.is_none());
    }

    #[test]
    fn dropping_driver_unregisters_sources_before_socket_and_clients_close() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        fixture.dispatch(Duration::from_millis(100));
        let path = fixture.driver.socket_path().to_owned();
        drop(fixture.driver);
        assert!(!path.exists());
        client
            .set_read_timeout(Some(Duration::from_millis(100)))
            .unwrap();
        let mut byte = [0];
        assert_eq!(client.read(&mut byte).unwrap(), 0);
        let wakes = fixture.state.wakes;
        fixture
            .event_loop
            .dispatch(Duration::from_millis(20), &mut fixture.state)
            .unwrap();
        assert_eq!(fixture.state.wakes, wakes);
    }

    #[test]
    fn driver_in_loop_owned_callback_does_not_form_strong_handle_cycle() {
        let temp = Temp::new();
        let event_loop = EventLoop::<State>::try_new().unwrap();
        let handle = event_loop.handle();
        let driver = EventLoopDriver::bind(&handle, &temp.0, "cycle", wake).unwrap();
        let path = driver.socket_path().to_owned();
        // A strong LoopHandle inside driver would keep this callback and driver
        // reachable from themselves after the external EventLoop is dropped.
        handle.insert_idle(move |_| drop(driver));
        drop(handle);
        drop(event_loop);
        assert!(!path.exists());
    }

    #[test]
    fn driver_can_outlive_loop_and_reports_broken_pipe_without_leaking_socket() {
        let temp = Temp::new();
        let event_loop = EventLoop::<State>::try_new().unwrap();
        let handle = event_loop.handle();
        let mut driver = EventLoopDriver::bind(&handle, &temp.0, "outlive", wake).unwrap();
        let path = driver.socket_path().to_owned();
        drop(handle);
        drop(event_loop);
        assert_eq!(
            driver
                .poll(Instant::now(), 0, |_| unreachable!())
                .unwrap_err()
                .kind(),
            io::ErrorKind::BrokenPipe
        );
        drop(driver);
        assert!(!path.exists());
    }

    #[test]
    fn stale_connection_registration_does_not_follow_reused_fd() {
        let mut fixture = Fixture::new();
        let client = fixture.client();
        fixture.dispatch(Duration::from_millis(100));
        let old_endpoint = *fixture
            .driver
            .registrations
            .keys()
            .find(|endpoint| matches!(endpoint, Endpoint::Client(_)))
            .unwrap();
        drop(client);
        fixture.dispatch(Duration::from_millis(100));
        assert!(!fixture.driver.registrations.contains_key(&old_endpoint));
        let mut next = fixture.client();
        next.write_all(&encode(&Request::new("new", Method::Version)).unwrap())
            .unwrap();
        fixture.dispatch(Duration::from_millis(100));
        assert!(!fixture.driver.registrations.contains_key(&old_endpoint));
        assert_eq!(read_response(&mut next).id, "new");
    }
    #[test]
    fn owned_sources_follow_write_backpressure_then_eof_for_large_reply() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        client
            .write_all(&encode(&Request::new("large", Method::Windows)).unwrap())
            .unwrap();
        client.shutdown(Shutdown::Write).unwrap();
        client.set_nonblocking(true).unwrap();
        let mut captured = Vec::new();
        let mut saw_write_interest = false;
        for _ in 0..40 {
            fixture
                .event_loop
                .dispatch(Duration::from_millis(50), &mut fixture.state)
                .unwrap();
            if std::mem::take(&mut fixture.state.pending) {
                fixture
                    .driver
                    .poll(Instant::now(), 0, |r| {
                        fixture.state.handled += 1;
                        Response::success(
                            r.id,
                            0,
                            serde_json::Value::String("x".repeat(crate::MAX_PAYLOAD / 2)),
                        )
                    })
                    .unwrap();
            }
            for registration in fixture.driver.registrations.values() {
                if registration.writable {
                    assert!(!registration.readable);
                    saw_write_interest = true;
                }
            }
            let mut bytes = [0; 8192];
            loop {
                match client.read(&mut bytes) {
                    Ok(0) => break,
                    Ok(n) => captured.extend_from_slice(&bytes[..n]),
                    Err(e) if e.kind() == io::ErrorKind::WouldBlock => break,
                    Err(e) => panic!("{e}"),
                }
            }
            if fixture.driver.connection_count() == 0 {
                break;
            }
        }
        assert!(saw_write_interest);
        assert_eq!(fixture.state.handled, 1);
        assert_eq!(fixture.driver.connection_count(), 0);
        assert_eq!(fixture.driver.registrations.len(), 1);
        let mut decoder = crate::Decoder::default();
        decoder.push(&captured).unwrap();
        let response: Response = decode(&decoder.next_frame().unwrap().unwrap()).unwrap();
        assert_eq!(response.id, "large");
        assert!(decoder.is_empty());
    }
    #[test]
    fn drain_deadline_wakes_without_reader_and_removes_all_socket_sources() {
        let mut fixture = Fixture::new();
        let mut client = fixture.client();
        client
            .write_all(&encode(&Request::new("quit", Method::Version)).unwrap())
            .unwrap();
        let now = Instant::now();
        let deadline = now + Duration::from_millis(30);
        fixture
            .driver
            .poll_controlled(now, 0, |r| Dispatch::Drain {
                response: Response::success(
                    r.id,
                    1,
                    serde_json::Value::String("x".repeat(crate::MAX_PAYLOAD / 2)),
                ),
                deadline,
            })
            .unwrap();
        assert!(matches!(
            fixture.driver.drain_state(),
            DrainState::Pending { .. }
        ));
        assert!(
            fixture
                .driver
                .registrations
                .values()
                .all(|r| !r.readable && r.writable)
        );
        let bound = Instant::now() + Duration::from_secs(1);
        while fixture.driver.drain_state() != DrainState::Drained {
            assert!(Instant::now() < bound);
            fixture.dispatch(Duration::from_millis(50));
        }
        assert_eq!(fixture.state.handled, 0);
        assert_eq!(fixture.driver.stats().drain_timed_out, 1);
        assert!(fixture.driver.registrations.is_empty());
        assert!(fixture.driver.deadline.is_none());
    }
}
