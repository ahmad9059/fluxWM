//! Nonfatal XDG shell responsiveness observations; no process liveness claim.
use calloop::{
    LoopHandle, RegistrationToken,
    timer::{TimeoutAction, Timer},
};
use serde_json::{Value, json};
use smithay::{utils::SERIAL_COUNTER, wayland::shell::xdg::ShellClient};
use std::{
    cell::Cell,
    io,
    rc::Rc,
    time::{Duration, Instant},
};

pub const TIMEOUT: Duration = Duration::from_secs(2);
pub const INTERVAL: Duration = Duration::from_secs(5);
const CAPACITY: usize = 1024;

#[derive(Clone, Copy, Debug)]
enum Probe {
    Initial(Instant),
    Ready(Instant),
    Pending { sent: Instant, expired: bool },
    Unavailable,
}
impl Probe {
    fn deadline(self) -> Option<Instant> {
        match self {
            Self::Initial(at) | Self::Ready(at) => Some(at),
            Self::Pending {
                sent,
                expired: false,
            } => Some(sent + TIMEOUT),
            _ => None,
        }
    }
    fn expire(&mut self, now: Instant) -> bool {
        if let Self::Pending { sent, expired } = self
            && !*expired
            && now >= *sent + TIMEOUT
        {
            *expired = true;
            true
        } else {
            false
        }
    }
    fn status(self) -> &'static str {
        match self {
            Self::Initial(_) => "unknown",
            Self::Ready(_) => "responsive",
            Self::Pending { expired: false, .. } => "waiting",
            Self::Pending { expired: true, .. } => "unresponsive",
            Self::Unavailable => "unavailable",
        }
    }
    /// A validated pong can arrive before its deadline callback is serviced.
    fn pong(&mut self, now: Instant) -> Option<(bool, bool)> {
        if let Self::Pending { expired, .. } = *self {
            let newly_expired = self.expire(now);
            let recovered = expired || newly_expired;
            *self = Self::Ready(now + INTERVAL);
            Some((newly_expired, recovered))
        } else {
            None
        }
    }
}
struct Entry {
    client: ShellClient,
    id: u64,
    probe: Probe,
}
#[derive(Default, serde::Serialize)]
struct Stats {
    sent: u64,
    pongs: u64,
    timeouts: u64,
    recoveries: u64,
    send_failures: u64,
    capacity_rejections: u64,
    timer_fired: u64,
}

pub struct Monitor<D: 'static> {
    entries: Vec<Entry>,
    next_id: u64,
    stats: Stats,
    dirty: bool,
    handle: Box<dyn Fn() -> Option<LoopHandle<'static, D>>>,
    timer: Option<(Instant, RegistrationToken)>,
    fired: Rc<Cell<bool>>,
    wake: fn(&mut D),
}
impl<D: 'static> Monitor<D> {
    pub fn new(handle: &LoopHandle<'static, D>, wake: fn(&mut D)) -> Self {
        let weak = handle.downgrade();
        Self {
            entries: Vec::new(),
            next_id: 1,
            stats: Stats::default(),
            dirty: false,
            handle: Box::new(move || weak.upgrade()),
            timer: None,
            fired: Rc::new(Cell::new(false)),
            wake,
        }
    }
    pub fn add(&mut self, client: ShellClient, now: Instant) {
        self.dirty = true;
        // This bounds our monitor, not upstream Wayland object admission.
        self.entries.retain(|e| e.client.alive());
        let Some(next) = self
            .next_id
            .checked_add(1)
            .filter(|_| self.entries.len() < CAPACITY)
        else {
            self.stats.capacity_rejections = self.stats.capacity_rejections.saturating_add(1);
            return;
        };
        self.entries.push(Entry {
            client,
            id: self.next_id,
            probe: Probe::Initial(now),
        });
        self.next_id = next;
    }
    pub fn remove(&mut self, client: &ShellClient) {
        let old_len = self.entries.len();
        self.entries.retain(|e| &e.client != client);
        self.dirty |= self.entries.len() != old_len;
    }
    pub fn pong(&mut self, client: &ShellClient, now: Instant) {
        let Some(entry) = self.entries.iter_mut().find(|e| &e.client == client) else {
            return;
        };
        if let Some((new_timeout, recovered)) = entry.probe.pong(now) {
            self.dirty = true;
            self.stats.pongs = self.stats.pongs.saturating_add(1);
            self.stats.timeouts = self.stats.timeouts.saturating_add(u64::from(new_timeout));
            self.stats.recoveries = self.stats.recoveries.saturating_add(u64::from(recovered));
        }
    }
    pub fn poll(&mut self, now: Instant, stopping: bool) -> io::Result<bool> {
        let mut changed = std::mem::take(&mut self.dirty);
        if self.fired.replace(false) {
            changed = true;
            self.stats.timer_fired = self.stats.timer_fired.saturating_add(1);
            self.timer = None;
        }
        let old_len = self.entries.len();
        self.entries.retain(|e| e.client.alive());
        if stopping {
            self.entries.clear();
        }
        changed |= old_len != self.entries.len();
        for entry in &mut self.entries {
            if entry.probe.expire(now) {
                changed = true;
                self.stats.timeouts = self.stats.timeouts.saturating_add(1);
            }
            if matches!(entry.probe, Probe::Initial(at) | Probe::Ready(at) if at <= now) {
                changed = true;
                if entry.client.send_ping(SERIAL_COUNTER.next_serial()).is_ok() {
                    entry.probe = Probe::Pending {
                        sent: now,
                        expired: false,
                    };
                    self.stats.sent = self.stats.sent.saturating_add(1);
                } else {
                    entry.probe = Probe::Unavailable;
                    self.stats.send_failures = self.stats.send_failures.saturating_add(1);
                }
            }
        }
        let deadline = self.entries.iter().filter_map(|e| e.probe.deadline()).min();
        let handle = (self.handle)().ok_or_else(|| {
            io::Error::new(io::ErrorKind::BrokenPipe, "shell health loop dropped")
        })?;
        if self.timer.as_ref().map(|t| t.0) != deadline {
            if let Some((_, token)) = self.timer.take() {
                handle.remove(token);
            }
            if let Some(at) = deadline {
                let fired = self.fired.clone();
                let wake = self.wake;
                let token = handle
                    .insert_source(Timer::from_deadline(at), move |_, _, state| {
                        fired.set(true);
                        wake(state);
                        TimeoutAction::Drop
                    })
                    .map_err(|e| io::Error::other(e.error.to_string()))?;
                self.timer = Some((at, token));
            }
        }
        Ok(changed)
    }
    pub fn for_client(&self, client: &ShellClient) -> Value {
        self.entries
            .iter()
            .find(|e| &e.client == client)
            .map_or_else(
                || json!({"status":"unmonitored"}),
                |e| json!({"id":e.id.to_string(),"status":e.probe.status()}),
            )
    }
    pub fn diagnostics(&self) -> Value {
        json!({"timeout_ms":TIMEOUT.as_millis(),"interval_ms":INTERVAL.as_millis(),"capacity":CAPACITY,
            "timer_armed":self.timer.is_some(),"stats":self.stats,
            "clients":self.entries.iter().map(|e| json!({"id":e.id.to_string(),"status":e.probe.status()})).collect::<Vec<_>>()})
    }
}
impl<D: 'static> Drop for Monitor<D> {
    fn drop(&mut self) {
        if let Some(handle) = (self.handle)()
            && let Some((_, token)) = self.timer.take()
        {
            handle.remove(token);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn timeout_is_once_at_boundary_and_late_pong_recovers_without_duplicate_probe() {
        let now = Instant::now();
        let mut probe = Probe::Pending {
            sent: now,
            expired: false,
        };
        assert!(!probe.expire(now + TIMEOUT - Duration::from_nanos(1)));
        assert!(probe.expire(now + TIMEOUT));
        assert_eq!(probe.status(), "unresponsive");
        assert!(!probe.expire(now + TIMEOUT + INTERVAL));
        assert_eq!(probe.deadline(), None);
        assert_eq!(probe.pong(now + TIMEOUT), Some((false, true)));
        assert_eq!(probe.status(), "responsive");
        assert_eq!(probe.deadline(), Some(now + TIMEOUT + INTERVAL));
        assert_eq!(probe.pong(now + TIMEOUT), None);
    }
    #[test]
    fn late_pong_before_timer_service_still_records_missed_deadline() {
        let now = Instant::now();
        let mut probe = Probe::Pending {
            sent: now,
            expired: false,
        };
        assert_eq!(probe.pong(now + TIMEOUT), Some((true, true)));
        let mut timely = Probe::Pending {
            sent: now,
            expired: false,
        };
        assert_eq!(
            timely.pong(now + TIMEOUT - Duration::from_nanos(1)),
            Some((false, false))
        );
    }
    #[test]
    fn empty_monitor_has_no_timer_or_strong_loop_cycle() {
        let mut event_loop = calloop::EventLoop::<()>::try_new().unwrap();
        let mut monitor = Monitor::new(&event_loop.handle(), |_| {});
        monitor.poll(Instant::now(), false).unwrap();
        assert!(!monitor.diagnostics()["timer_armed"].as_bool().unwrap());
        event_loop.dispatch(Duration::ZERO, &mut ()).unwrap();
        drop(event_loop);
        assert_eq!(
            monitor.poll(Instant::now(), false).unwrap_err().kind(),
            io::ErrorKind::BrokenPipe
        );
    }
}
