//! Owned live primary-plane output. No implicit modeset occurs on Drop.
use super::{
    SeatDevice,
    restore::RestoreState,
    target_test::{self, Target},
};
use crate::{
    Ticket,
    frames::{self, FramePool, FrameQueue, IdleProof, OutputKey, SubmitError, SubmitOutcome},
    output::{Assignment, Format},
};
use serde::Serialize;
use smithay::{
    backend::{
        allocator::gbm::GbmDevice,
        drm::{DrmDevice, DrmDeviceFd},
        renderer::{
            Renderer,
            gles::{GlesRenderer, GlesTarget},
            sync::SyncPoint,
        },
        session::libseat::LibSeatSession,
    },
    reexports::drm::control,
    utils::{Physical, Size},
};
use std::{any::Any, collections::BTreeMap, io, os::fd::AsFd, rc::Rc};

#[derive(Clone, Copy, Debug, Serialize)]
pub enum PreviewStage {
    Inactive,
    Busy,
    Admission,
    Prepare,
    Render,
    EventStream,
    Capture,
    Test,
    Submit,
    Restore,
    Cleanup,
}
#[derive(Clone, Copy, Debug, Serialize)]
pub struct PreviewFailure {
    pub stage: PreviewStage,
    pub errno: Option<i32>,
}
impl PreviewFailure {
    pub(super) fn at(stage: PreviewStage) -> Self {
        Self { stage, errno: None }
    }
    fn io(stage: PreviewStage, error: io::Error) -> Self {
        Self {
            stage,
            errno: error.raw_os_error(),
        }
    }
}
impl std::fmt::Display for PreviewFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "native preview failed: {:?} ({:?})",
            self.stage, self.errno
        )
    }
}
impl std::error::Error for PreviewFailure {}
#[derive(Debug, Serialize)]
pub struct PreviewInfo {
    pub connector: u32,
    pub queue: frames::QueueSnapshot,
    pub visible_color: Option<[u8; 4]>,
    pub last_failure: Option<PreviewFailure>,
    pub rollback_completed: bool,
    pub rollback_cleanup_errors: u64,
}
pub(super) struct Context<'a> {
    pub drm: &'a DrmDevice,
    pub gbm: &'a GbmDevice<DrmDeviceFd>,
    pub renderer: &'a mut GlesRenderer,
    pub modes: &'a BTreeMap<u32, Vec<control::Mode>>,
    pub seat: Rc<SeatDevice<LibSeatSession>>,
    pub pool: &'a FramePool,
    pub ticket: Ticket,
}
struct OwnedTarget {
    target: Target,
    _seat: Rc<SeatDevice<LibSeatSession>>,
}
enum Scene {
    Color([u8; 4]),
    Content(Rc<dyn Any>),
}
impl Scene {
    fn content<T: 'static>(&self) -> Option<Rc<T>> {
        match self {
            Self::Content(scene) => scene.clone().downcast().ok(),
            Self::Color(_) => None,
        }
    }
}
struct State {
    queue: FrameQueue<OwnedTarget, Scene>,
    rollback: RestoreState,
    fd: DrmDeviceFd,
    assignment: Assignment,
    format: Format,
    key: OutputKey,
    last_failure: Option<PreviewFailure>,
    rollback_completed: bool,
    armed: bool,
    // Pin broker originals until all cloned framebuffer/DRM owners have retired.
    _seat: Rc<SeatDevice<LibSeatSession>>,
}
pub(super) struct Output {
    state: Option<Box<State>>,
}
fn color(value: [u8; 4]) -> [f32; 4] {
    value.map(|channel| f32::from(channel) / 255.)
}
fn bytes(a: &Assignment) -> Result<u64, PreviewFailure> {
    let pixels = u64::from(a.mode.width) * u64::from(a.mode.height);
    if a.mode.width == 0
        || a.mode.height == 0
        || a.mode.width > crate::target::MAX_TEST_DIMENSION
        || a.mode.height > crate::target::MAX_TEST_DIMENSION
        || pixels > crate::target::MAX_TEST_PIXELS
    {
        return Err(PreviewFailure::at(PreviewStage::Admission));
    }
    Ok(pixels * 4)
}
impl Output {
    pub fn begin(
        ctx: &mut Context<'_>,
        a: Assignment,
        format: Format,
        epoch: u64,
        value: [u8; 4],
    ) -> Result<Self, PreviewFailure> {
        let key = OutputKey::new(ctx.ticket, a.crtc, epoch)
            .map_err(|_| PreviewFailure::at(PreviewStage::Admission))?;
        let mut queue = ctx
            .pool
            .output(key)
            .map_err(|_| PreviewFailure::at(PreviewStage::Admission))?;
        let mut request = None;
        let mut rollback = None;
        let frame = ctx
            .pool
            .prepare(bytes(&a)?, || {
                let (target, initial) = target_test::prepare_live(
                    ctx.drm,
                    ctx.gbm,
                    ctx.renderer,
                    ctx.modes,
                    &a,
                    format,
                    color(value),
                )
                .map_err(|e| PreviewFailure {
                    stage: PreviewStage::Prepare,
                    errno: e.errno,
                })?;
                let saved = RestoreState::capture(ctx.drm, &initial)
                    .map_err(|e| PreviewFailure::io(PreviewStage::Capture, e))?;
                // Both the new configuration AND rollback must pass preflight.
                initial
                    .test_only(ctx.drm.as_fd())
                    .map_err(|e| PreviewFailure::io(PreviewStage::Test, e))?;
                saved
                    .request()
                    .test_only(ctx.drm.as_fd())
                    .map_err(|e| PreviewFailure::io(PreviewStage::Test, e))?;
                request = Some(initial);
                rollback = Some(saved);
                Ok((
                    OwnedTarget {
                        target,
                        _seat: ctx.seat.clone(),
                    },
                    Scene::Color(value),
                ))
            })
            .map_err(|e| match e {
                frames::PreparationError::Backend(e) => e,
                _ => PreviewFailure::at(PreviewStage::Admission),
            });
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                ctx.renderer
                    .cleanup_texture_cache()
                    .map_err(|_| PreviewFailure::at(PreviewStage::Cleanup))?;
                return Err(error);
            }
        };
        let initial = request.expect("prepared initial request");
        let result = queue.submit(frame, |_, _| {
            match initial.apply_blocking(ctx.drm.as_fd()) {
                Ok(()) => SubmitOutcome::Accepted,
                // Never infer that an ioctl error proves no consumer exists.
                Err(error) => {
                    SubmitOutcome::Uncertain(PreviewFailure::io(PreviewStage::Submit, error))
                }
            }
        });
        let last_failure = match result {
            Ok(cookie) => {
                // SAFETY: the initial ALLOW_MODESET ioctl returned synchronously
                // without NONBLOCK or events; no previous queue consumer exists.
                unsafe {
                    queue.complete_blocking(cookie);
                }
                None
            }
            Err(SubmitError::Uncertain(error)) => Some(error),
            Err(error) => {
                drop(error);
                ctx.renderer
                    .cleanup_texture_cache()
                    .map_err(|_| PreviewFailure::at(PreviewStage::Cleanup))?;
                return Err(PreviewFailure::at(PreviewStage::Admission));
            }
        };
        if last_failure.is_some() {
            // A failed initial ioctl is not a startup-stream quiescence proof.
            // Even a colliding old cookie must never reopen frame admission.
            queue.invalidate();
        }
        Ok(Self {
            state: Some(Box::new(State {
                queue,
                rollback: rollback.expect("prepared rollback"),
                fd: ctx.drm.device_fd().clone(),
                assignment: a,
                format,
                key,
                last_failure,
                rollback_completed: false,
                armed: true,
                _seat: ctx.seat.clone(),
            })),
        })
    }
    pub fn info(&self) -> PreviewInfo {
        let s = self.state.as_ref().expect("live output owned");
        PreviewInfo {
            connector: s.assignment.connector,
            queue: s.queue.snapshot(),
            visible_color: s.queue.visible_scene().and_then(|scene| match scene {
                Scene::Color(color) => Some(*color),
                Scene::Content(_) => None,
            }),
            last_failure: s.last_failure,
            rollback_completed: s.rollback_completed,
            rollback_cleanup_errors: s.rollback.cleanup_errors(),
        }
    }
    pub fn complete(&mut self, ticket: Ticket, event: crate::drm_events::Flip) {
        let s = self.state.as_mut().expect("live output owned");
        if s.queue.complete(ticket, event) == frames::Completion::Presented {
            s.last_failure = None;
        }
    }
    pub fn invalidate(&mut self) {
        self.state
            .as_mut()
            .expect("live output owned")
            .queue
            .invalidate();
    }
    pub fn content<T: 'static>(&self) -> Option<Rc<T>> {
        self.state.as_ref()?.queue.visible_scene()?.content()
    }
    pub fn submit(&mut self, ctx: &mut Context<'_>, value: [u8; 4]) -> Result<(), PreviewFailure> {
        self.submit_with(ctx, value, |_, _| Ok(Scene::Color(value)))
    }
    pub fn render<T: 'static>(
        &mut self,
        ctx: &mut Context<'_>,
        compose: impl FnOnce(
            &mut GlesRenderer,
            &mut GlesTarget<'_>,
            Size<i32, Physical>,
        ) -> Result<(Rc<T>, SyncPoint), String>,
    ) -> Result<(), PreviewFailure> {
        self.submit_with(ctx, [0, 0, 0, 255], |renderer, target| {
            target
                .compose(renderer, compose)
                .map(|scene| Scene::Content(scene))
                .map_err(|failure| PreviewFailure {
                    stage: PreviewStage::Render,
                    errno: failure.errno,
                })
        })
    }
    fn submit_with(
        &mut self,
        ctx: &mut Context<'_>,
        value: [u8; 4],
        compose: impl FnOnce(&mut GlesRenderer, &mut Target) -> Result<Scene, PreviewFailure>,
    ) -> Result<(), PreviewFailure> {
        let s = self.state.as_mut().expect("live output owned");
        let status = s.queue.snapshot();
        if !status.active || status.closed || status.faulted || s.rollback_completed {
            return Err(PreviewFailure::at(PreviewStage::Inactive));
        }
        if status.pending_cookie.is_some() {
            return Err(PreviewFailure::at(PreviewStage::Busy));
        }
        let prepared = ctx
            .pool
            .prepare(bytes(&s.assignment)?, || {
                let (mut target, _) = target_test::prepare_live(
                    ctx.drm,
                    ctx.gbm,
                    ctx.renderer,
                    ctx.modes,
                    &s.assignment,
                    s.format,
                    color(value),
                )
                .map_err(|e| PreviewFailure {
                    stage: PreviewStage::Prepare,
                    errno: e.errno,
                })?;
                let scene = compose(ctx.renderer, &mut target)?;
                Ok((
                    OwnedTarget {
                        target,
                        _seat: ctx.seat.clone(),
                    },
                    scene,
                ))
            })
            .map_err(|e| match e {
                frames::PreparationError::Backend(e) => e,
                _ => PreviewFailure::at(PreviewStage::Admission),
            });
        let prepared = match prepared {
            Ok(frame) => frame,
            Err(error) => {
                ctx.renderer
                    .cleanup_texture_cache()
                    .map_err(|_| PreviewFailure::at(PreviewStage::Cleanup))?;
                return Err(error);
            }
        };
        let result = s.queue.submit(prepared, |cookie, owned| {
            let request = match owned.target.flip_request() {
                Ok(r) => r,
                Err(e) => {
                    return SubmitOutcome::Rejected(PreviewFailure {
                        stage: PreviewStage::Prepare,
                        errno: e.errno,
                    });
                }
            };
            match request.submit(ctx.drm.as_fd(), cookie) {
                Ok(()) => SubmitOutcome::Accepted,
                Err(e) => SubmitOutcome::Uncertain(PreviewFailure::io(PreviewStage::Submit, e)),
            }
        });
        match result {
            Ok(_) => Ok(()),
            Err(SubmitError::Uncertain(error)) => {
                s.last_failure = Some(error);
                Err(error)
            }
            Err(SubmitError::Rejected { error, frame }) => {
                drop(frame);
                ctx.renderer
                    .cleanup_texture_cache()
                    .map_err(|_| PreviewFailure::at(PreviewStage::Cleanup))?;
                s.last_failure = Some(error);
                Err(error)
            }
            Err(error) => {
                drop(error);
                ctx.renderer
                    .cleanup_texture_cache()
                    .map_err(|_| PreviewFailure::at(PreviewStage::Cleanup))?;
                Err(PreviewFailure::at(PreviewStage::Admission))
            }
        }
    }
    pub fn restore(&mut self) -> Result<(), PreviewFailure> {
        let s = self.state.as_mut().expect("live output owned");
        s.queue.invalidate();
        let rollback = &mut s.rollback;
        restore_consumers(&mut s.queue, s.key, &mut s.rollback_completed, || {
            rollback.mark_restored();
            rollback.request().apply_blocking(s.fd.as_fd())
        })?;
        s.rollback
            .finish_restored()
            .map_err(|e| PreviewFailure::io(PreviewStage::Cleanup, e))?;
        s.armed = false;
        Ok(())
    }
}
impl Drop for Output {
    fn drop(&mut self) {
        if let Some(state) = self.state.take()
            && state.armed
        {
            // No restore against a potentially revoked/replaced topology. Keep
            // resources, rollback clones, quota permits AND the broker lease.
            std::mem::forget(state);
        }
    }
}

fn restore_consumers<R, S>(
    queue: &mut FrameQueue<R, S>,
    key: OutputKey,
    completed: &mut bool,
    restore: impl FnOnce() -> io::Result<()>,
) -> Result<(), PreviewFailure> {
    queue.invalidate();
    if !*completed {
        restore().map_err(|e| PreviewFailure::io(PreviewStage::Restore, e))?;
        // SAFETY: the platform closure succeeded only after a synchronous
        // atomic rollback replaced every plane consuming this queue's buffers.
        let proof = unsafe { IdleProof::after_backend_quiescence(key) };
        queue
            .release_after_disable(proof)
            .map_err(|_| PreviewFailure::at(PreviewStage::Restore))?;
        *completed = true;
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn composed_content_stays_private_until_exact_completion_and_stops_on_invalidation() {
        let ticket = Ticket {
            device: 1,
            generation: 1,
            session_epoch: 1,
        };
        let key = OutputKey::new(ticket, 7, 1).unwrap();
        let pool = FramePool::default();
        let mut queue = pool.output(key).unwrap();
        let expected = Rc::new(String::from("composed scene"));
        let frame = pool
            .prepare(4, || Ok::<_, ()>(((), Scene::Content(expected.clone()))))
            .unwrap();
        let cookie = queue
            .submit(frame, |_, _| SubmitOutcome::<()>::Accepted)
            .ok()
            .unwrap();
        assert!(queue.visible_scene().is_none());
        let mut flip = crate::drm_events::Flip {
            cookie: cookie.get(),
            crtc: 8,
            sequence: 0,
            seconds: 0,
            microseconds: 0,
        };
        assert_eq!(queue.complete(ticket, flip), frames::Completion::Ignored);
        assert!(queue.visible_scene().is_none());
        flip.crtc = 7;
        assert_eq!(queue.complete(ticket, flip), frames::Completion::Presented);
        let actual = queue.visible_scene().unwrap().content::<String>().unwrap();
        assert!(Rc::ptr_eq(&actual, &expected));
        assert!(queue.visible_scene().unwrap().content::<u64>().is_none());
        queue.invalidate();
        assert!(queue.visible_scene().is_none());
        // SAFETY: this fixture has no hardware consumers.
        queue
            .release_after_disable(unsafe { IdleProof::after_backend_quiescence(key) })
            .unwrap();
        assert_eq!(pool.snapshot().slots, 0);
    }
    #[test]
    fn failed_restore_retains_front_and_pending_until_synchronous_success() {
        let ticket = Ticket {
            device: 1,
            generation: 1,
            session_epoch: 1,
        };
        let key = OutputKey::new(ticket, 7, 1).unwrap();
        let pool = FramePool::default();
        let mut queue = pool.output(key).unwrap();
        let frame = pool.prepare(4, || Ok::<_, ()>((1, ()))).unwrap();
        let cookie = queue
            .submit(frame, |_, _| SubmitOutcome::<()>::Accepted)
            .ok()
            .unwrap();
        queue.complete(
            ticket,
            crate::drm_events::Flip {
                cookie: cookie.get(),
                crtc: 7,
                sequence: 1,
                seconds: 0,
                microseconds: 0,
            },
        );
        let next = pool.prepare(4, || Ok::<_, ()>((2, ()))).unwrap();
        assert!(
            queue
                .submit(next, |_, _| SubmitOutcome::<()>::Accepted)
                .is_ok()
        );
        let mut completed = false;
        assert!(
            restore_consumers(&mut queue, key, &mut completed, || Err(
                io::Error::from_raw_os_error(libc::EBUSY)
            ))
            .is_err()
        );
        assert_eq!(pool.snapshot().slots, 2);
        assert!(!completed);
        assert!(queue.visible_scene().is_none());
        restore_consumers(&mut queue, key, &mut completed, || {
            assert_eq!(pool.snapshot().slots, 2);
            Ok(())
        })
        .unwrap();
        assert_eq!(pool.snapshot().slots, 0);
        assert!(queue.snapshot().closed && completed);
        restore_consumers(&mut queue, key, &mut completed, || {
            panic!("rollback must not repeat after clone IDs close")
        })
        .unwrap();
    }
}
