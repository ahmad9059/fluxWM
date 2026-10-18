use serde::Serialize;
use std::collections::BTreeMap;

pub const MAX_DEVICES: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Ticket {
    pub device: u64,
    pub generation: u64,
    pub session_epoch: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    Stopped,
    Inactive,
    UnknownDevice,
    Capacity,
    Exhausted,
    NotPending,
    Stale,
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Stopped => "native lifecycle is stopped",
            Self::Inactive => "native session is inactive",
            Self::UnknownDevice => "native device is not registered",
            Self::Capacity => "native device capacity exceeded",
            Self::Exhausted => "native lifecycle generation exhausted",
            Self::NotPending => "native device has no pending preparation",
            Self::Stale => "native preparation ticket is stale",
        })
    }
}
impl std::error::Error for Error {}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeviceState {
    Discovered,
    Preparing,
    Ready,
    Failed,
    Suspended,
}
struct Device {
    generation: u64,
    state: DeviceState,
}
#[derive(Clone, Debug, Serialize)]
pub struct DeviceSnapshot {
    pub device: String,
    pub generation: String,
    pub state: DeviceState,
}
#[derive(Clone, Debug, Serialize)]
pub struct Snapshot {
    pub active: bool,
    pub closed: bool,
    pub session_epoch: String,
    pub devices: Vec<DeviceSnapshot>,
}
#[derive(Default)]
pub struct Lifecycle {
    active: bool,
    closed: bool,
    epoch: u64,
    generation: u64,
    devices: BTreeMap<u64, Device>,
}
impl Lifecycle {
    fn open(&self) -> Result<(), Error> {
        if self.closed {
            Err(Error::Stopped)
        } else {
            Ok(())
        }
    }
    fn next_generation(&mut self) -> Result<u64, Error> {
        let Some(next) = self.generation.checked_add(1) else {
            self.stop();
            return Err(Error::Exhausted);
        };
        self.generation = next;
        Ok(next)
    }
    fn next_epoch(&mut self) -> Result<(), Error> {
        let Some(next) = self.epoch.checked_add(1) else {
            self.stop();
            return Err(Error::Exhausted);
        };
        self.epoch = next;
        Ok(())
    }
    /// Duplicate discovery is idempotent; Changed explicitly invalidates state.
    pub fn discover(&mut self, device: u64) -> Result<bool, Error> {
        self.open()?;
        if self.devices.contains_key(&device) {
            return Ok(false);
        }
        if self.devices.len() == MAX_DEVICES {
            return Err(Error::Capacity);
        }
        let generation = self.next_generation()?;
        self.devices.insert(
            device,
            Device {
                generation,
                state: DeviceState::Discovered,
            },
        );
        Ok(true)
    }
    pub fn changed(&mut self, device: u64) -> Result<(), Error> {
        self.open()?;
        if !self.devices.contains_key(&device) {
            return Err(Error::UnknownDevice);
        }
        let generation = self.next_generation()?;
        let entry = self.devices.get_mut(&device).expect("checked device");
        entry.generation = generation;
        entry.state = DeviceState::Discovered;
        Ok(())
    }
    pub fn remove(&mut self, device: u64) -> Result<bool, Error> {
        self.open()?;
        Ok(self.devices.remove(&device).is_some())
    }
    pub fn activate(&mut self) -> Result<bool, Error> {
        self.open()?;
        if self.active {
            return Ok(false);
        }
        self.next_epoch()?;
        self.active = true;
        for entry in self.devices.values_mut() {
            entry.state = DeviceState::Discovered;
        }
        Ok(true)
    }
    pub fn pause(&mut self) -> Result<bool, Error> {
        self.open()?;
        if !self.active {
            return Ok(false);
        }
        self.next_epoch()?;
        self.active = false;
        for entry in self.devices.values_mut() {
            entry.state = DeviceState::Suspended;
        }
        Ok(true)
    }
    /// Failed work retries only after explicit change or a session transition.
    pub fn begin(&mut self, device: u64) -> Result<Ticket, Error> {
        self.open()?;
        if !self.active {
            return Err(Error::Inactive);
        }
        let entry = self.devices.get_mut(&device).ok_or(Error::UnknownDevice)?;
        if entry.state != DeviceState::Discovered {
            return Err(Error::NotPending);
        }
        entry.state = DeviceState::Preparing;
        Ok(Ticket {
            device,
            generation: entry.generation,
            session_epoch: self.epoch,
        })
    }
    pub fn is_current(&self, ticket: Ticket) -> bool {
        !self.closed
            && self.active
            && ticket.session_epoch == self.epoch
            && self.devices.get(&ticket.device).is_some_and(|d| {
                d.generation == ticket.generation && d.state == DeviceState::Preparing
            })
    }
    pub fn is_ready(&self, ticket: Ticket) -> bool {
        !self.closed
            && self.active
            && ticket.session_epoch == self.epoch
            && self
                .devices
                .get(&ticket.device)
                .is_some_and(|d| d.generation == ticket.generation && d.state == DeviceState::Ready)
    }
    pub fn complete(&mut self, ticket: Ticket, success: bool) -> Result<(), Error> {
        if !self.is_current(ticket) {
            return Err(Error::Stale);
        }
        self.devices
            .get_mut(&ticket.device)
            .expect("current device")
            .state = if success {
            DeviceState::Ready
        } else {
            DeviceState::Failed
        };
        Ok(())
    }
    pub fn closed(&self) -> bool {
        self.closed
    }
    pub fn active(&self) -> bool {
        self.active && !self.closed
    }
    /// Stop cannot fail, including after counter exhaustion.
    pub fn stop(&mut self) {
        self.closed = true;
        self.active = false;
        for entry in self.devices.values_mut() {
            entry.state = DeviceState::Suspended;
        }
    }
    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            active: self.active(),
            closed: self.closed,
            session_epoch: self.epoch.to_string(),
            devices: self
                .devices
                .iter()
                .map(|(id, d)| DeviceSnapshot {
                    device: id.to_string(),
                    generation: d.generation.to_string(),
                    state: d.state,
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inactive_boot_and_duplicate_discovery_cannot_prepare() {
        let mut s = Lifecycle::default();
        assert!(s.discover(7).unwrap());
        let before = s.snapshot().devices[0].generation.clone();
        assert!(!s.discover(7).unwrap());
        assert_eq!(s.begin(7), Err(Error::Inactive));
        assert_eq!(s.snapshot().devices[0].generation, before);
        s.activate().unwrap();
        let t = s.begin(7).unwrap();
        s.complete(t, true).unwrap();
        assert_eq!(s.begin(7), Err(Error::NotPending));
        assert!(!s.activate().unwrap());
        assert_eq!(s.snapshot().devices[0].state, DeviceState::Ready);
    }
    #[test]
    fn pause_resume_and_changed_state_reject_old_completion() {
        let mut s = Lifecycle::default();
        s.activate().unwrap();
        s.discover(1).unwrap();
        let a = s.begin(1).unwrap();
        s.pause().unwrap();
        assert_eq!(s.complete(a, true), Err(Error::Stale));
        assert!(!s.pause().unwrap());
        s.activate().unwrap();
        let b = s.begin(1).unwrap();
        assert_ne!(a.session_epoch, b.session_epoch);
        assert!(!s.is_current(a));
        assert_eq!(s.complete(a, true), Err(Error::Stale));
        assert!(s.is_current(b));
        s.changed(1).unwrap();
        assert_eq!(s.complete(b, true), Err(Error::Stale));
        let c = s.begin(1).unwrap();
        s.complete(c, false).unwrap();
        assert_eq!(s.begin(1), Err(Error::NotPending));
        s.changed(1).unwrap();
        let d = s.begin(1).unwrap();
        s.complete(d, true).unwrap();
        assert!(s.is_ready(d));
        s.pause().unwrap();
        s.activate().unwrap();
        let e = s.begin(1).unwrap();
        s.complete(e, true).unwrap();
        assert!(!s.is_ready(d));
        assert!(s.is_ready(e));
    }
    #[test]
    fn remove_and_reused_device_number_do_not_reuse_ticket() {
        let mut s = Lifecycle::default();
        s.activate().unwrap();
        s.discover(9).unwrap();
        let a = s.begin(9).unwrap();
        assert!(s.remove(9).unwrap());
        assert!(!s.remove(9).unwrap());
        s.discover(9).unwrap();
        let b = s.begin(9).unwrap();
        assert_ne!(a.generation, b.generation);
        assert_eq!(s.complete(a, true), Err(Error::Stale));
        s.complete(b, true).unwrap();
    }
    #[test]
    fn capacity_failure_preserves_ready_devices_and_removal_releases_capacity() {
        let mut s = Lifecycle::default();
        s.activate().unwrap();
        for id in 0..MAX_DEVICES as u64 {
            s.discover(id).unwrap();
            let t = s.begin(id).unwrap();
            s.complete(t, true).unwrap();
        }
        let before = s.generation;
        assert_eq!(s.discover(100), Err(Error::Capacity));
        assert_eq!(s.generation, before);
        assert!(
            s.snapshot()
                .devices
                .iter()
                .all(|d| d.state == DeviceState::Ready)
        );
        s.remove(0).unwrap();
        s.discover(100).unwrap();
    }
    #[test]
    fn exhausted_counters_seal_instead_of_wrapping() {
        let mut s = Lifecycle::default();
        s.activate().unwrap();
        s.discover(1).unwrap();
        let t = s.begin(1).unwrap();
        s.generation = u64::MAX;
        assert_eq!(s.discover(2), Err(Error::Exhausted));
        assert!(!s.is_current(t));
        assert_eq!(s.activate(), Err(Error::Stopped));
        s.stop();
        let mut s = Lifecycle {
            epoch: u64::MAX,
            ..Default::default()
        };
        assert_eq!(s.activate(), Err(Error::Exhausted));
        assert!(s.snapshot().closed);
    }
    #[test]
    fn shutdown_invalidates_pending_and_all_future_work() {
        let mut s = Lifecycle::default();
        s.activate().unwrap();
        s.discover(1).unwrap();
        let t = s.begin(1).unwrap();
        s.stop();
        assert_eq!(s.complete(t, true), Err(Error::Stale));
        assert_eq!(s.discover(2), Err(Error::Stopped));
        assert_eq!(s.remove(1), Err(Error::Stopped));
        assert_eq!(s.changed(1), Err(Error::Stopped));
        assert!(s.snapshot().closed);
        assert!(!s.snapshot().active);
    }
}
