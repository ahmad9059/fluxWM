//! Owned rollback values for exactly the properties a commit will change.
//! Capture must run while the caller serializes KMS operations on this device.
use super::atomic::{MAX_OBJECTS, MAX_PROPERTIES, Request};
use smithay::{
    backend::drm::{DrmDevice, DrmDeviceFd},
    reexports::{
        drm::control::{self, Device as _, ResourceHandle, property::ValueType},
        rustix::ioctl::opcode,
    },
};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    os::fd::{AsFd, AsRawFd},
};

const MAX_BLOB_BYTES: usize = 1024 * 1024;
const MAX_TOTAL_BLOB_BYTES: usize = 4 * 1024 * 1024;

pub(super) struct RestoreState {
    request: Request,
    fd: Option<DrmDeviceFd>,
    blobs: Vec<u32>,
    blob_bytes: usize,
    framebuffers: Vec<control::framebuffer::Handle>,
    restored: bool,
    cleanup_errors: u64,
}
impl RestoreState {
    pub fn capture(drm: &DrmDevice, changed: &Request) -> io::Result<Self> {
        let resources = drm.resource_handles()?;
        let planes = drm.plane_handles()?;
        let mut objects = BTreeMap::new();
        for &h in resources.connectors() {
            insert_object(&mut objects, h)?;
        }
        for &h in resources.crtcs() {
            insert_object(&mut objects, h)?;
        }
        for &h in &planes {
            insert_object(&mut objects, h)?;
        }
        let mut snapshot = Self {
            request: Request::new(),
            fd: Some(drm.device_fd().clone()),
            blobs: Vec::new(),
            blob_bytes: 0,
            framebuffers: Vec::new(),
            restored: false,
            cleanup_errors: 0,
        };
        let mut old_blobs = BTreeMap::new();
        let mut old_fbs = BTreeMap::new();
        let mut properties = BTreeMap::new();
        let mut count = 0;
        for (object, property, _) in changed.entries() {
            count += 1;
            if count > MAX_PROPERTIES {
                return Err(invalid("too many rollback properties"));
            }
            let kind = *objects
                .get(&object)
                .ok_or_else(|| invalid("unknown rollback object"))?;
            if let std::collections::btree_map::Entry::Vacant(entry) = properties.entry(object) {
                let values = match kind {
                    control::connector::Handle::FFI_TYPE => {
                        drm.get_properties(handle::<control::connector::Handle>(object)?)?
                    }
                    control::crtc::Handle::FFI_TYPE => {
                        drm.get_properties(handle::<control::crtc::Handle>(object)?)?
                    }
                    control::plane::Handle::FFI_TYPE => {
                        drm.get_properties(handle::<control::plane::Handle>(object)?)?
                    }
                    _ => return Err(invalid("unsupported rollback object")),
                };
                if values.as_props_and_values().0.len() > MAX_PROPERTIES {
                    return Err(invalid("too many object properties"));
                }
                let mut current = BTreeMap::new();
                for (id, value) in values {
                    if current.insert(u32::from(id), value).is_some() {
                        return Err(invalid("duplicate object property"));
                    }
                }
                entry.insert(current);
            }
            let value = *properties[&object]
                .get(&property)
                .ok_or_else(|| invalid("property missing on rollback object"))?;
            let info = drm.get_property(handle::<control::property::Handle>(property)?)?;
            if !info.mutable() {
                return Err(invalid("immutable rollback property"));
            }
            let name = info.name().to_bytes();
            if transient(name) || !supported(kind, name) {
                return Err(invalid("unsupported or transient rollback property"));
            }
            let value = match info.value_type() {
                ValueType::Blob if value != 0 => {
                    let original = u32::try_from(value).map_err(|_| invalid("invalid blob ID"))?;
                    if let Some(&id) = old_blobs.get(&original) {
                        id
                    } else {
                        let id = snapshot.clone_blob(original)?;
                        old_blobs.insert(original, id);
                        id
                    }
                }
                ValueType::Framebuffer if value != 0 => {
                    if name != b"FB_ID" || kind != control::plane::Handle::FFI_TYPE {
                        return Err(invalid("unsupported framebuffer property"));
                    }
                    let original =
                        u32::try_from(value).map_err(|_| invalid("invalid framebuffer ID"))?;
                    if let Some(&id) = old_fbs.get(&original) {
                        id
                    } else {
                        let id = snapshot.clone_framebuffer(original)?;
                        old_fbs.insert(original, id);
                        id
                    }
                }
                ValueType::Unknown
                | ValueType::Object
                | ValueType::Property
                | ValueType::Encoder => return Err(invalid("unsupported property type")),
                _ => value,
            };
            snapshot
                .request
                .add(object, property, value)
                .map_err(|_| invalid("rollback request capacity"))?;
        }
        if count == 0 {
            return Err(invalid("empty rollback request"));
        }
        Ok(snapshot)
    }
    pub fn request(&self) -> &Request {
        &self.request
    }
    /// Mark before issuing rollback: an ioctl error may still be ambiguous.
    /// This permanently prevents RMFB cleanup of possibly restored scanout.
    /// The owner must retain the full backend/seat lease on restoration error.
    pub fn mark_restored(&mut self) {
        self.restored = true;
    }
    /// After synchronous rollback succeeds, release clone ownership without
    /// disabling restored planes. Failures retain FB IDs and the fd for retry;
    /// the caller must retain this entire state and its backend/seat owner.
    pub fn finish_restored(&mut self) -> io::Result<()> {
        if !self.restored {
            return Err(invalid("rollback not marked restored"));
        }
        let before = self.cleanup_errors;
        self.cleanup();
        if !self.framebuffers.is_empty() || !self.blobs.is_empty() || self.cleanup_errors != before
        {
            return Err(io::Error::other("rollback resource cleanup failed"));
        }
        Ok(())
    }
    pub fn cleanup_errors(&self) -> u64 {
        self.cleanup_errors
    }
    fn fd(&self) -> &DrmDeviceFd {
        self.fd.as_ref().expect("snapshot owns its DRM fd")
    }
    fn clone_blob(&mut self, original: u32) -> io::Result<u64> {
        let mut get = drm_sys::drm_mode_get_blob {
            blob_id: original,
            ..Default::default()
        };
        ioctl(self.fd(), 0xac, &mut get)?;
        let length = get.length as usize;
        if length == 0
            || length > MAX_BLOB_BYTES
            || self.blob_bytes.saturating_add(length) > MAX_TOTAL_BLOB_BYTES
        {
            return Err(invalid("rollback blob size limit"));
        }
        let mut bytes = vec![0u8; length];
        get.data = bytes.as_mut_ptr() as u64;
        ioctl(self.fd(), 0xac, &mut get)?;
        if get.length as usize != length {
            return Err(invalid("rollback blob changed size"));
        }
        let mut create = drm_sys::drm_mode_create_blob {
            data: bytes.as_ptr() as u64,
            length: length as u32,
            blob_id: 0,
        };
        ioctl(self.fd(), 0xbd, &mut create)?;
        if create.blob_id == 0 {
            return Err(invalid("zero cloned blob ID"));
        }
        self.blobs.push(create.blob_id);
        self.blob_bytes += length;
        Ok(create.blob_id.into())
    }
    fn clone_framebuffer(&mut self, original: u32) -> io::Result<u64> {
        let mut raw = drm_sys::drm_mode_fb_cmd2 {
            fb_id: original,
            ..Default::default()
        };
        // On failure the kernel already deletes any newly created handles,
        // even when their stale numbers have been copied back to userspace.
        ioctl(self.fd(), 0xce, &mut raw)?;
        // GETFB2 gives fresh handles, potentially repeated across planes. Install
        // the guard before validating anything returned by the kernel.
        let mut handles = Handles::new(raw.handles);
        handles.fd = Some(self.fd().clone());
        let result = (|| {
            if raw.width == 0 || raw.height == 0 || raw.handles[0] == 0 {
                return Err(invalid("framebuffer unavailable for rollback"));
            }
            if raw.flags & !control::FbCmd2Flags::all().bits() != 0 {
                return Err(invalid("unknown framebuffer flags"));
            }
            // Probe CLOSEFB on an unattached clone before retaining anything
            // that could later become restored scanout. Keep GETFB2 handles
            // alive across both ADDFB2 calls so the exact backing is reused.
            let fd = self.fd().clone();
            let id = qualified_clone(|operation| match operation {
                CloneOperation::Create => {
                    raw.fb_id = 0;
                    ioctl(&fd, 0xb8, &mut raw)?;
                    let fb = handle::<control::framebuffer::Handle>(raw.fb_id)?;
                    self.framebuffers.push(fb);
                    Ok(raw.fb_id)
                }
                CloneOperation::Close(id) => {
                    let mut close = drm_sys::drm_mode_closefb { fb_id: id, pad: 0 };
                    ioctl(&fd, 0xd0, &mut close)?;
                    self.framebuffers.retain(|fb| u32::from(*fb) != id);
                    Ok(0)
                }
                CloneOperation::Remove(id) => {
                    fd.destroy_framebuffer(handle(id)?)?;
                    self.framebuffers.retain(|fb| u32::from(*fb) != id);
                    Ok(0)
                }
            })?;
            Ok(u64::from(id))
        })();
        let close = handles.close(|id| self.fd().close_buffer(handle(id)?));
        if close > 0 {
            self.cleanup_errors = self.cleanup_errors.saturating_add(close);
            return Err(io::Error::other("failed to close rollback GEM handle"));
        }
        result
    }
}
impl RestoreState {
    /// Release userspace leases. After restoration CLOSEFB preserves scanout;
    /// unsupported/error kernels retain the file description rather than RMFB.
    /// Returns the cumulative count of failed cleanup operations.
    pub fn cleanup(&mut self) -> u64 {
        let mut retained_blobs = Vec::new();
        for blob in self.blobs.drain(..) {
            if self
                .fd
                .as_ref()
                .unwrap()
                .destroy_property_blob(blob.into())
                .is_err()
            {
                self.cleanup_errors = self.cleanup_errors.saturating_add(1);
                retained_blobs.push(blob);
            }
        }
        self.blobs = retained_blobs;
        let mut retained = Vec::new();
        for fb in self.framebuffers.drain(..) {
            let result = if self.restored {
                let mut close = drm_sys::drm_mode_closefb {
                    fb_id: fb.into(),
                    pad: 0,
                };
                ioctl(self.fd.as_ref().unwrap(), 0xd0, &mut close)
            } else {
                self.fd.as_ref().unwrap().destroy_framebuffer(fb)
            };
            if result.is_err() {
                self.cleanup_errors = self.cleanup_errors.saturating_add(1);
                retained.push(fb);
            }
        }
        self.framebuffers = retained;
        self.cleanup_errors
    }
}
impl Drop for RestoreState {
    fn drop(&mut self) {
        self.cleanup();
    }
}
fn handle<H: From<control::RawResourceHandle>>(value: u32) -> io::Result<H> {
    std::num::NonZeroU32::new(value)
        .map(H::from)
        .ok_or_else(|| invalid("zero resource handle"))
}
fn insert_object<H: ResourceHandle>(objects: &mut BTreeMap<u32, u32>, handle: H) -> io::Result<()> {
    let raw: control::RawResourceHandle = handle.into();
    if objects.len() == MAX_OBJECTS || objects.insert(raw.get(), H::FFI_TYPE).is_some() {
        return Err(invalid("duplicate or excessive KMS objects"));
    }
    Ok(())
}
fn invalid(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
// This adapter changes only this qualified property set. New driver-specific
// properties need an explicit lifetime/side-effect review before adding them.
fn supported(kind: u32, name: &[u8]) -> bool {
    match kind {
        control::connector::Handle::FFI_TYPE => name == b"CRTC_ID",
        control::crtc::Handle::FFI_TYPE => matches!(name, b"ACTIVE" | b"MODE_ID"),
        control::plane::Handle::FFI_TYPE => matches!(
            name,
            b"FB_ID"
                | b"CRTC_ID"
                | b"SRC_X"
                | b"SRC_Y"
                | b"SRC_W"
                | b"SRC_H"
                | b"CRTC_X"
                | b"CRTC_Y"
                | b"CRTC_W"
                | b"CRTC_H"
                | b"rotation"
                | b"alpha"
        ),
        _ => false,
    }
}
fn transient(name: &[u8]) -> bool {
    name.windows(5).any(|part| part == b"FENCE")
        || name.starts_with(b"WRITEBACK_")
        || name == b"FB_DAMAGE_CLIPS"
}
fn ioctl<T>(fd: &DrmDeviceFd, number: u8, data: &mut T) -> io::Result<()> {
    // SAFETY: private callers select matching pinned repr(C) DRM structures;
    // all pointer-bearing buffers remain live for this synchronous ioctl.
    let result = unsafe {
        libc::ioctl(
            fd.as_fd().as_raw_fd(),
            opcode::read_write::<T>(b'd', number) as libc::c_ulong,
            data as *mut T,
        )
    };
    if result < 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CloneOperation {
    Create,
    Close(u32),
    Remove(u32),
}
fn qualified_clone(mut call: impl FnMut(CloneOperation) -> io::Result<u32>) -> io::Result<u32> {
    let probe = call(CloneOperation::Create)?;
    if let Err(error) = call(CloneOperation::Close(probe)) {
        // The probe has never been attached to a plane, so RMFB is safe.
        // Failed cleanup stays recorded in the snapshot for its Drop retry.
        let _ = call(CloneOperation::Remove(probe));
        return Err(error);
    }
    call(CloneOperation::Create)
}
struct Handles {
    remaining: BTreeSet<u32>,
    fd: Option<DrmDeviceFd>,
}
impl Handles {
    fn new(handles: [u32; 4]) -> Self {
        Self {
            remaining: handles.into_iter().filter(|&id| id != 0).collect(),
            fd: None,
        }
    }
    fn close(&mut self, mut close: impl FnMut(u32) -> io::Result<()>) -> u64 {
        let mut errors = 0;
        while let Some(id) = self.remaining.pop_first() {
            if close(id).is_err() {
                errors += 1;
            }
        }
        errors
    }
}
impl Drop for Handles {
    fn drop(&mut self) {
        if let Some(fd) = self.fd.take() {
            self.close(|id| fd.close_buffer(handle(id)?));
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn failed_closefb_preflight_cleans_probe_without_retained_clone() {
        let mut calls = Vec::new();
        let result = qualified_clone(|operation| {
            calls.push(operation);
            match operation {
                CloneOperation::Create => Ok(17),
                CloneOperation::Close(_) => Err(io::Error::from_raw_os_error(libc::ENOTTY)),
                CloneOperation::Remove(_) => Ok(0),
            }
        });
        assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::ENOTTY));
        assert_eq!(
            calls,
            [
                CloneOperation::Create,
                CloneOperation::Close(17),
                CloneOperation::Remove(17)
            ]
        );
    }
    #[test]
    fn successful_closefb_preflight_precedes_retained_clone() {
        let mut calls = Vec::new();
        let mut id = 16;
        let result = qualified_clone(|operation| {
            calls.push(operation);
            match operation {
                CloneOperation::Create => {
                    id += 1;
                    Ok(id)
                }
                CloneOperation::Close(_) => Ok(0),
                CloneOperation::Remove(_) => panic!("successful probe needs no RMFB"),
            }
        });
        assert_eq!(result.unwrap(), 18);
        assert_eq!(
            calls,
            [
                CloneOperation::Create,
                CloneOperation::Close(17),
                CloneOperation::Create
            ]
        );
    }
    #[test]
    fn distinct_getfb_handles_closed_once_even_when_close_fails() {
        let mut handles = Handles::new([4, 4, 0, 7]);
        let mut seen = Vec::new();
        assert_eq!(
            handles.close(|id| {
                seen.push(id);
                Err(io::Error::other("fixture"))
            }),
            2
        );
        assert_eq!(seen, [4, 7]);
        assert_eq!(handles.close(|_| panic!("closed twice")), 0);
    }
    #[test]
    fn unknown_and_wrong_object_properties_are_rejected() {
        assert!(supported(control::plane::Handle::FFI_TYPE, b"FB_ID"));
        assert!(supported(control::crtc::Handle::FFI_TYPE, b"MODE_ID"));
        assert!(!supported(control::connector::Handle::FFI_TYPE, b"FB_ID"));
        assert!(!supported(
            control::plane::Handle::FFI_TYPE,
            b"vendor_magic"
        ));
    }
    #[test]
    fn transient_values_are_rejected() {
        for name in [
            b"IN_FENCE_FD".as_slice(),
            b"OUT_FENCE_PTR",
            b"WRITEBACK_FB_ID",
            b"FB_DAMAGE_CLIPS",
        ] {
            assert!(transient(name));
        }
        for name in [b"MODE_ID".as_slice(), b"FB_ID", b"CRTC_ID", b"ACTIVE"] {
            assert!(!transient(name));
        }
    }
}
