//! Manual atomic TEST_ONLY adapter. No DrmSurface or output manager is created:
//! their active teardown/legacy test paths can change real display state.
use crate::{
    output::{Assignment, Format, Plan},
    target::{self, Backend, TestFailure, TestReport, TestStage},
};
use smithay::{
    backend::{
        allocator::{
            Allocator, Buffer, Fourcc, Modifier,
            dmabuf::{AsDmabuf, Dmabuf},
            gbm::{GbmAllocator, GbmBuffer, GbmBufferFlags, GbmDevice},
        },
        drm::{DrmDevice, DrmDeviceFd},
        renderer::{Bind, Frame, Renderer, gles::GlesRenderer},
    },
    reexports::drm::control::{
        self, AtomicCommitFlags, Device as _, FbCmd2Flags, ResourceHandle, atomic::AtomicModeReq,
    },
    utils::{Rectangle, Transform},
};
use std::{cell::Cell, collections::BTreeMap, rc::Rc};
fn fail(stage: TestStage) -> TestFailure {
    TestFailure::at(stage)
}
fn io(stage: TestStage, error: std::io::Error) -> TestFailure {
    TestFailure {
        stage,
        errno: error.raw_os_error(),
    }
}
#[derive(Clone, Copy)]
enum Object {
    Framebuffer(control::framebuffer::Handle),
    Blob(u64),
}
struct Lease {
    fd: DrmDeviceFd,
    object: Object,
    failures: Rc<Cell<u64>>,
}
impl Drop for Lease {
    fn drop(&mut self) {
        let result = match self.object {
            Object::Framebuffer(fb) => self.fd.destroy_framebuffer(fb),
            Object::Blob(blob) => self.fd.destroy_property_blob(blob),
        };
        if result.is_err() {
            self.failures.set(self.failures.get().saturating_add(1));
        }
    }
}
struct Properties {
    object: control::RawResourceHandle,
    values: BTreeMap<String, (control::property::Handle, u64)>,
}
impl Properties {
    fn read<H: ResourceHandle>(drm: &DrmDevice, handle: H) -> Result<Self, TestFailure> {
        let values = drm
            .get_properties(handle)
            .map_err(|e| io(TestStage::Properties, e))?;
        if values.as_props_and_values().0.len() > 256 {
            return Err(fail(TestStage::Properties));
        }
        let mut map = BTreeMap::new();
        for (id, value) in values {
            let info = drm
                .get_property(id)
                .map_err(|e| io(TestStage::Properties, e))?;
            let name = info
                .name()
                .to_str()
                .map_err(|_| fail(TestStage::Properties))?
                .to_owned();
            if map.insert(name, (id, value)).is_some() {
                return Err(fail(TestStage::Properties));
            }
        }
        Ok(Self {
            object: handle.into(),
            values: map,
        })
    }
    fn set(&self, request: &mut AtomicModeReq, name: &str, value: u64) -> Result<(), TestFailure> {
        let (property, _) = self
            .values
            .get(name)
            .ok_or_else(|| fail(TestStage::Properties))?;
        request.add_raw_property(self.object, *property, value);
        Ok(())
    }
    fn optional(&self, request: &mut AtomicModeReq, name: &str, value: u64) {
        if let Some((property, _)) = self.values.get(name) {
            request.add_raw_property(self.object, *property, value);
        }
    }
}
struct Target {
    // Framebuffer and blob retire before DMA-BUF/GBM storage on every path.
    framebuffer: Lease,
    mode_blob: Lease,
    _dmabuf: Dmabuf,
    _buffer: GbmBuffer,
    assignment: Assignment,
    crtc: Properties,
    plane: Properties,
}
struct Adapter<'a> {
    drm: &'a DrmDevice,
    renderer: &'a mut GlesRenderer,
    allocator: GbmAllocator<DrmDeviceFd>,
    modes: &'a BTreeMap<u32, Vec<control::Mode>>,
    failures: Rc<Cell<u64>>,
}
fn handle<H: ResourceHandle>(id: u32) -> Result<H, TestFailure> {
    control::from_u32(id).ok_or_else(|| fail(TestStage::StaleTopology))
}
/// Kept private, with no caller-supplied flags or live-commit entry point.
fn flags() -> AtomicCommitFlags {
    AtomicCommitFlags::ALLOW_MODESET
}
impl Backend for Adapter<'_> {
    type Target = Target;
    fn atomic(&self) -> bool {
        self.drm.is_atomic()
    }
    fn cleanup_failures(&self) -> u64 {
        self.failures.get()
    }
    fn finish_cleanup(&mut self) -> Result<(), TestFailure> {
        self.renderer
            .cleanup_texture_cache()
            .map_err(|_| fail(TestStage::Cleanup))
    }
    fn prepare(&mut self, a: &Assignment, format: Format) -> Result<Target, TestFailure> {
        if !self.drm.is_active() {
            return Err(fail(TestStage::Inactive));
        }
        let connector = handle::<control::connector::Handle>(a.connector)?;
        let encoder = handle::<control::encoder::Handle>(a.encoder)?;
        let crtc = handle::<control::crtc::Handle>(a.crtc)?;
        let plane = handle::<control::plane::Handle>(a.primary_plane)?;
        let mode = *self
            .modes
            .get(&a.connector)
            .and_then(|m| m.get(a.mode_index))
            .ok_or_else(|| fail(TestStage::StaleTopology))?;
        let live = self
            .drm
            .get_connector(connector, false)
            .map_err(|e| io(TestStage::StaleTopology, e))?;
        if live.state() != control::connector::State::Connected
            || !live.modes().contains(&mode)
            || !live.encoders().contains(&encoder)
        {
            return Err(fail(TestStage::StaleTopology));
        }
        let resources = self
            .drm
            .resource_handles()
            .map_err(|e| io(TestStage::StaleTopology, e))?;
        if resources.crtcs().len() > crate::output::MAX_CRTCS {
            return Err(fail(TestStage::StaleTopology));
        }
        let encoder_info = self
            .drm
            .get_encoder(encoder)
            .map_err(|e| io(TestStage::StaleTopology, e))?;
        if !resources
            .filter_crtcs(encoder_info.possible_crtcs())
            .contains(&crtc)
        {
            return Err(fail(TestStage::StaleTopology));
        }
        let native_format = smithay::backend::allocator::Format {
            code: Fourcc::try_from(format.code).map_err(|_| fail(TestStage::AllocatedFormat))?,
            modifier: Modifier::from(format.modifier),
        };
        let primary = self
            .drm
            .planes(&crtc)
            .map_err(|_| fail(TestStage::StaleTopology))?
            .primary;
        if !primary
            .iter()
            .any(|p| p.handle == plane && p.formats.contains(&native_format))
        {
            return Err(fail(TestStage::StaleTopology));
        }
        let connector_properties = Properties::read(self.drm, connector)?;
        if connector_properties
            .values
            .get("non-desktop")
            .is_some_and(|(_, value)| *value != 0)
        {
            return Err(fail(TestStage::StaleTopology));
        }
        let crtc_properties = Properties::read(self.drm, crtc)?;
        let plane_properties = Properties::read(self.drm, plane)?;
        let buffer = self
            .allocator
            .create_buffer(
                u32::from(a.mode.width),
                u32::from(a.mode.height),
                native_format.code,
                &[native_format.modifier],
            )
            .map_err(|e| io(TestStage::Allocate, e))?;
        // GBM may fall back from an explicit modifier to implicit allocation.
        // Such a buffer must never be tested under the requested explicit pair.
        if buffer.format() != native_format
            || buffer.width() != u32::from(a.mode.width)
            || buffer.height() != u32::from(a.mode.height)
        {
            return Err(fail(TestStage::AllocatedFormat));
        }
        let fb_flags = if native_format.modifier == Modifier::Invalid {
            FbCmd2Flags::empty()
        } else {
            FbCmd2Flags::MODIFIERS
        };
        let framebuffer = self
            .drm
            .add_planar_framebuffer(&buffer, fb_flags)
            .map_err(|e| io(TestStage::Export, e))?;
        let framebuffer = Lease {
            fd: self.drm.device_fd().clone(),
            object: Object::Framebuffer(framebuffer),
            failures: self.failures.clone(),
        };
        let mut dmabuf = buffer.export().map_err(|_| fail(TestStage::Export))?;
        let sync = {
            let mut target = self
                .renderer
                .bind(&mut dmabuf)
                .map_err(|_| fail(TestStage::Bind))?;
            let size = (i32::from(a.mode.width), i32::from(a.mode.height)).into();
            let mut frame = self
                .renderer
                .render(&mut target, size, Transform::Normal)
                .map_err(|_| fail(TestStage::Render))?;
            frame
                .clear([0., 0., 0., 1.].into(), &[Rectangle::from_size(size)])
                .map_err(|_| fail(TestStage::Render))?;
            frame.finish().map_err(|_| fail(TestStage::Render))?
        };
        let mut reached = false;
        for _ in 0..8 {
            if sync.wait().is_ok() {
                reached = true;
                break;
            }
        }
        if !reached {
            return Err(fail(TestStage::Sync));
        }
        let blob = self
            .drm
            .create_property_blob(&mode)
            .map_err(|e| io(TestStage::ModeBlob, e))?
            .as_blob()
            .ok_or_else(|| fail(TestStage::ModeBlob))?;
        let mode_blob = Lease {
            fd: self.drm.device_fd().clone(),
            object: Object::Blob(blob),
            failures: self.failures.clone(),
        };
        Ok(Target {
            framebuffer,
            mode_blob,
            _dmabuf: dmabuf,
            _buffer: buffer,
            assignment: a.clone(),
            crtc: crtc_properties,
            plane: plane_properties,
        })
    }
    fn test_only(&mut self, targets: &[Target]) -> Result<(), TestFailure> {
        if !self.drm.is_active() || !self.drm.is_atomic() {
            return Err(fail(TestStage::Inactive));
        }
        use crate::target::routes::{Route, reconcile};
        let resources = self
            .drm
            .resource_handles()
            .map_err(|e| io(TestStage::StaleTopology, e))?;
        if resources.connectors().len() > crate::output::MAX_CONNECTORS
            || resources.crtcs().len() > crate::output::MAX_CRTCS
        {
            return Err(fail(TestStage::Budget));
        }
        let mut connector_properties = BTreeMap::new();
        let mut connector_routes = vec![];
        for connector in resources.connectors() {
            let properties = Properties::read(self.drm, *connector)?;
            let value = properties
                .values
                .get("CRTC_ID")
                .ok_or_else(|| fail(TestStage::Properties))?
                .1;
            let crtc = u32::try_from(value).map_err(|_| fail(TestStage::StaleTopology))?;
            connector_routes.push(Route {
                id: (*connector).into(),
                crtc: (crtc != 0).then_some(crtc),
            });
            connector_properties.insert(u32::from(*connector), properties);
        }
        let planes = self
            .drm
            .plane_handles()
            .map_err(|e| io(TestStage::Properties, e))?;
        if planes.len() > 256 {
            return Err(fail(TestStage::Budget));
        }
        let mut plane_routes = vec![];
        for plane in planes {
            let info = self
                .drm
                .get_plane(plane)
                .map_err(|e| io(TestStage::Properties, e))?;
            plane_routes.push(Route {
                id: plane.into(),
                crtc: info.crtc().map(u32::from),
            });
        }
        let assignments: Vec<_> = targets
            .iter()
            .map(|target| target.assignment.clone())
            .collect();
        let crtcs: Vec<_> = resources.crtcs().iter().map(|id| u32::from(*id)).collect();
        let reconciled = reconcile(&assignments, &connector_routes, &plane_routes, &crtcs)?;
        let mut request = AtomicModeReq::new();
        for (connector, crtc) in &reconciled.connectors {
            connector_properties[connector].set(
                &mut request,
                "CRTC_ID",
                u64::from(crtc.unwrap_or(0)),
            )?;
        }
        for crtc in &reconciled.disabled_crtcs {
            let properties = Properties::read(self.drm, handle::<control::crtc::Handle>(*crtc)?)?;
            properties.set(&mut request, "ACTIVE", 0)?;
            properties.set(&mut request, "MODE_ID", 0)?;
        }
        for target in targets {
            let a = &target.assignment;
            if connector_properties[&a.connector]
                .values
                .get("non-desktop")
                .is_some_and(|(_, value)| *value != 0)
            {
                return Err(fail(TestStage::StaleTopology));
            }
            let Object::Framebuffer(fb) = target.framebuffer.object else {
                unreachable!()
            };
            let Object::Blob(blob) = target.mode_blob.object else {
                unreachable!()
            };
            target.crtc.set(&mut request, "ACTIVE", 1)?;
            target.crtc.set(&mut request, "MODE_ID", blob)?;
            for (name, value) in [
                ("FB_ID", u64::from(u32::from(fb))),
                ("CRTC_ID", u64::from(a.crtc)),
                ("SRC_X", 0),
                ("SRC_Y", 0),
                ("SRC_W", u64::from(a.mode.width) << 16),
                ("SRC_H", u64::from(a.mode.height) << 16),
                ("CRTC_X", 0),
                ("CRTC_Y", 0),
                ("CRTC_W", u64::from(a.mode.width)),
                ("CRTC_H", u64::from(a.mode.height)),
            ] {
                target.plane.set(&mut request, name, value)?;
            }
            target.plane.optional(&mut request, "rotation", 1);
            target.plane.optional(&mut request, "alpha", 65535);
        }
        for plane in &reconciled.cleared_planes {
            let properties = Properties::read(self.drm, handle::<control::plane::Handle>(*plane)?)?;
            properties.set(&mut request, "FB_ID", 0)?;
            properties.set(&mut request, "CRTC_ID", 0)?;
        }
        self.drm
            .atomic_commit(flags(), request)
            .map_err(|e| io(TestStage::AtomicTest, e))
    }
}
pub(super) fn test(
    drm: &DrmDevice,
    gbm: &GbmDevice<DrmDeviceFd>,
    renderer: &mut GlesRenderer,
    modes: &BTreeMap<u32, Vec<control::Mode>>,
    plan: &Plan,
) -> TestReport {
    let mut adapter = Adapter {
        drm,
        renderer,
        allocator: GbmAllocator::new(
            gbm.clone(),
            GbmBufferFlags::RENDERING | GbmBufferFlags::SCANOUT,
        ),
        modes,
        failures: Rc::new(Cell::new(0)),
    };
    target::run(plan, &mut adapter)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn kernel_request_flags_always_test_only_without_events_or_nonblock() {
        assert_eq!(
            flags(),
            AtomicCommitFlags::TEST_ONLY | AtomicCommitFlags::ALLOW_MODESET
        );
        assert!(!flags().intersects(
            AtomicCommitFlags::NONBLOCK
                | AtomicCommitFlags::PAGE_FLIP_EVENT
                | AtomicCommitFlags::PAGE_FLIP_ASYNC
        ));
    }
}
