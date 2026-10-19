//! Bounded cookie-preserving serialization of the pinned Linux atomic UAPI.
//! Live submission requires an owned frame cookie; rollback is synchronous.
//! Public driver integration owns buffers through completion and restoration.
use smithay::reexports::{drm::control::AtomicCommitFlags, rustix::ioctl::opcode};
use std::{
    collections::BTreeMap,
    io,
    os::fd::{AsRawFd, BorrowedFd},
};
pub(super) const MAX_OBJECTS: usize = 352;
pub(super) const MAX_PROPERTIES: usize = 2048;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Error {
    ZeroHandle,
    Capacity,
}
#[derive(Default)]
pub(super) struct Request {
    objects: BTreeMap<u32, BTreeMap<u32, u64>>,
    properties: usize,
}
struct Arrays {
    objects: Vec<u32>,
    counts: Vec<u32>,
    properties: Vec<u32>,
    values: Vec<u64>,
}
impl Request {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn add(&mut self, object: u32, property: u32, value: u64) -> Result<(), Error> {
        if object == 0 || property == 0 {
            return Err(Error::ZeroHandle);
        }
        if let Some(properties) = self.objects.get_mut(&object) {
            if let Some(old) = properties.get_mut(&property) {
                *old = value;
                return Ok(());
            }
        } else if self.objects.len() == MAX_OBJECTS {
            return Err(Error::Capacity);
        }
        if self.properties == MAX_PROPERTIES {
            return Err(Error::Capacity);
        }
        self.objects
            .entry(object)
            .or_default()
            .insert(property, value);
        self.properties += 1;
        Ok(())
    }
    pub fn entries(&self) -> impl Iterator<Item = (u32, u32, u64)> + '_ {
        self.objects.iter().flat_map(|(object, props)| {
            props
                .iter()
                .map(move |(property, value)| (*object, *property, *value))
        })
    }
    fn with_raw<T>(
        &self,
        flags: u32,
        cookie: u64,
        call: impl FnOnce(&mut drm_sys::drm_mode_atomic, &Arrays) -> T,
    ) -> T {
        let mut arrays = Arrays {
            objects: Vec::with_capacity(self.objects.len()),
            counts: Vec::with_capacity(self.objects.len()),
            properties: Vec::with_capacity(self.properties),
            values: Vec::with_capacity(self.properties),
        };
        for (object, properties) in &self.objects {
            arrays.objects.push(*object);
            arrays.counts.push(properties.len() as u32);
            for (property, value) in properties {
                arrays.properties.push(*property);
                arrays.values.push(*value);
            }
        }
        let mut raw = drm_sys::drm_mode_atomic {
            flags,
            count_objs: arrays.objects.len() as u32,
            objs_ptr: arrays.objects.as_mut_ptr() as u64,
            count_props_ptr: arrays.counts.as_mut_ptr() as u64,
            props_ptr: arrays.properties.as_mut_ptr() as u64,
            prop_values_ptr: arrays.values.as_mut_ptr() as u64,
            reserved: 0,
            user_data: cookie,
        };
        // Kernel request-array copying is synchronous even for NONBLOCK;
        // all pointees stay owned and unchanged through this call.
        call(&mut raw, &arrays)
    }
    pub fn test_only(&self, fd: BorrowedFd<'_>) -> io::Result<()> {
        if self.objects.is_empty() {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        self.issue(fd, test_flags(), 0)
    }
    /// A single-output commit. The queue retains resources before this call.
    pub fn submit(&self, fd: BorrowedFd<'_>, cookie: crate::frames::Cookie) -> io::Result<()> {
        self.issue(fd, flip_flags(), cookie.get())
    }
    /// Blocking replacement of every property changed by the owned output.
    /// Successful return is the platform quiescence boundary for those planes.
    pub fn apply_blocking(&self, fd: BorrowedFd<'_>) -> io::Result<()> {
        self.issue(fd, blocking_flags(), 0)
    }
    fn issue(&self, fd: BorrowedFd<'_>, flags: u32, cookie: u64) -> io::Result<()> {
        if self.objects.is_empty() {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        self.with_raw(flags, cookie, |raw, _| {
            // SAFETY: pinned repr(C) UAPI and locally owned, bounded live arrays.
            // DRM copies request arrays synchronously even with NONBLOCK; only
            // the scalar cookie survives into completion, never these pointers.
            let result = unsafe {
                libc::ioctl(
                    fd.as_raw_fd(),
                    atomic_opcode() as libc::c_ulong,
                    raw as *mut drm_sys::drm_mode_atomic,
                )
            };
            if result < 0 {
                Err(io::Error::last_os_error())
            } else {
                Ok(())
            }
        })
    }
}
pub(super) fn atomic_opcode() -> smithay::reexports::rustix::ioctl::Opcode {
    opcode::read_write::<drm_sys::drm_mode_atomic>(b'd', 0xbc)
}
fn flip_flags() -> u32 {
    (AtomicCommitFlags::NONBLOCK | AtomicCommitFlags::PAGE_FLIP_EVENT).bits()
}
fn blocking_flags() -> u32 {
    AtomicCommitFlags::ALLOW_MODESET.bits()
}
fn test_flags() -> u32 {
    (AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET).bits()
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn live_commits_require_events_but_initial_modeset_and_rollback_do_not() {
        let flip = AtomicCommitFlags::from_bits(flip_flags()).unwrap();
        assert!(flip.contains(AtomicCommitFlags::NONBLOCK | AtomicCommitFlags::PAGE_FLIP_EVENT));
        assert!(!flip.intersects(
            AtomicCommitFlags::ALLOW_MODESET
                | AtomicCommitFlags::TEST_ONLY
                | AtomicCommitFlags::PAGE_FLIP_ASYNC
        ));
        let blocking = AtomicCommitFlags::from_bits(blocking_flags()).unwrap();
        assert!(blocking.contains(AtomicCommitFlags::ALLOW_MODESET));
        // Off-to-off CRTCs reject requested events even after TEST_ONLY passes.
        assert!(!blocking.intersects(
            AtomicCommitFlags::NONBLOCK
                | AtomicCommitFlags::PAGE_FLIP_EVENT
                | AtomicCommitFlags::TEST_ONLY
        ));
    }
    #[test]
    fn full_cookie_pointer_arrays_order_and_replacement_are_exact() {
        let mut request = Request::new();
        request.add(9, 3, 4).unwrap();
        request.add(2, 8, 7).unwrap();
        request.add(9, 1, 2).unwrap();
        request.add(9, 3, u64::MAX).unwrap();
        request.with_raw(test_flags(), u64::MAX, |raw, arrays| {
            assert_eq!(raw.user_data, u64::MAX);
            assert_eq!(raw.reserved, 0);
            assert_eq!(raw.count_objs, 2);
            assert_eq!(arrays.objects, [2, 9]);
            assert_eq!(arrays.counts, [1, 2]);
            assert_eq!(arrays.properties, [8, 1, 3]);
            assert_eq!(arrays.values, [7, 2, u64::MAX]);
            assert_eq!(raw.objs_ptr, arrays.objects.as_ptr() as u64);
            assert_eq!(raw.count_props_ptr, arrays.counts.as_ptr() as u64);
            assert_eq!(raw.props_ptr, arrays.properties.as_ptr() as u64);
            assert_eq!(raw.prop_values_ptr, arrays.values.as_ptr() as u64);
        });
    }
    #[test]
    fn kernel_request_flags_always_test_only_without_events_or_nonblock() {
        assert_eq!(
            test_flags(),
            (AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET).bits()
        );
        assert_eq!(
            test_flags()
                & (AtomicCommitFlags::PAGE_FLIP_EVENT
                    | AtomicCommitFlags::NONBLOCK
                    | AtomicCommitFlags::PAGE_FLIP_ASYNC)
                    .bits(),
            0
        );
    }
    #[test]
    fn capacity_rejection_is_atomic_and_replacement_remains_possible() {
        let mut request = Request::new();
        assert_eq!(request.add(0, 1, 1), Err(Error::ZeroHandle));
        assert_eq!(request.add(1, 0, 1), Err(Error::ZeroHandle));
        for property in 1..=MAX_PROPERTIES as u32 {
            request.add(1, property, 1).unwrap();
        }
        assert_eq!(request.add(2, 1, 1), Err(Error::Capacity));
        assert_eq!(request.objects.len(), 1);
        assert_eq!(request.properties, MAX_PROPERTIES);
        request.add(1, 1, 2).unwrap();
        let mut request = Request::new();
        for object in 1..=MAX_OBJECTS as u32 {
            request.add(object, 1, 1).unwrap();
        }
        assert_eq!(
            request.add(MAX_OBJECTS as u32 + 1, 1, 1),
            Err(Error::Capacity)
        );
        assert_eq!(request.properties, MAX_OBJECTS);
    }
    #[test]
    fn uapi_layout_matches_current_linux_struct_contract() {
        assert_eq!(std::mem::size_of::<drm_sys::drm_mode_atomic>(), 56);
        assert_eq!(
            std::mem::offset_of!(drm_sys::drm_mode_atomic, user_data),
            48
        );
        assert_eq!(std::mem::size_of::<drm_sys::drm_event_vblank>(), 32);
        assert_eq!(
            std::mem::offset_of!(drm_sys::drm_event_vblank, user_data),
            8
        );
        assert_eq!(std::mem::offset_of!(drm_sys::drm_event_vblank, crtc_id), 28);
        println!(
            "fluxwm-drm-abi:{}",
            serde_json::json!({
                "atomic_size":std::mem::size_of::<drm_sys::drm_mode_atomic>(),
                "atomic_cookie_offset":std::mem::offset_of!(drm_sys::drm_mode_atomic,user_data),
                "event_size":std::mem::size_of::<drm_sys::drm_event_vblank>(),
                "event_cookie_offset":std::mem::offset_of!(drm_sys::drm_event_vblank,user_data),
                "event_crtc_offset":std::mem::offset_of!(drm_sys::drm_event_vblank,crtc_id),
                "atomic_ioctl":atomic_opcode() as u64,
                "test_flags":test_flags(),
            })
        );
    }
}
