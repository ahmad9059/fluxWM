//! Owned scanout-consumer ledger. Platform code must finish rendering/acquire
//! before preparing a frame and supply genuine cookie-bearing DRM completions.
//! No KMS submitter or live compositor integration is provided by this module.
use crate::Ticket;
use serde::Serialize;
use std::{
    cell::RefCell,
    collections::BTreeSet,
    rc::Rc,
    sync::atomic::{AtomicU64, Ordering},
};
pub const MAX_FRAME_SLOTS: usize = 64;
pub const MAX_FRAME_BYTES: u64 = 256 * 1024 * 1024;
pub const MAX_OUTPUTS: usize = 32;
static LAST_COOKIE: AtomicU64 = AtomicU64::new(0);
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct Cookie(#[serde(serialize_with = "decimal")] u64);
impl Cookie {
    pub fn get(self) -> u64 {
        self.0
    }
}
fn decimal<S: serde::Serializer>(value: &u64, serializer: S) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&value.to_string())
}
fn allocate_cookie(counter: &AtomicU64) -> Result<Cookie, Error> {
    counter
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |old| {
            old.checked_add(1)
        })
        .map(|old| Cookie(old + 1))
        .map_err(|_| Error::Exhausted)
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct OutputKey {
    #[serde(serialize_with = "decimal")]
    device: u64,
    #[serde(serialize_with = "decimal")]
    generation: u64,
    #[serde(serialize_with = "decimal")]
    session_epoch: u64,
    crtc: u32,
    #[serde(serialize_with = "decimal")]
    output_epoch: u64,
}
impl OutputKey {
    pub fn new(ticket: Ticket, crtc: u32, output_epoch: u64) -> Result<Self, Error> {
        if ticket.generation == 0 || ticket.session_epoch == 0 || crtc == 0 || output_epoch == 0 {
            return Err(Error::InvalidKey);
        }
        Ok(Self {
            device: ticket.device,
            generation: ticket.generation,
            session_epoch: ticket.session_epoch,
            crtc,
            output_epoch,
        })
    }
    fn source_matches(self, source: Ticket, crtc: u32) -> bool {
        self.device == source.device
            && self.generation == source.generation
            && self.session_epoch == source.session_epoch
            && self.crtc == crtc
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidKey,
    Capacity,
    DuplicateOutput,
    WrongPool,
    Inactive,
    Pending,
    Exhausted,
    WrongProof,
}
#[derive(Default)]
struct Budget {
    slots: usize,
    bytes: u64,
    orphaned: usize,
    orphaned_bytes: u64,
    outputs: BTreeSet<OutputKey>,
}
#[derive(Clone, Default)]
pub struct FramePool {
    state: Rc<RefCell<Budget>>,
}
#[derive(Debug, PartialEq, Eq, Serialize)]
pub struct PoolSnapshot {
    pub slots: usize,
    pub nominal_bytes: String,
    pub orphaned: usize,
    pub orphaned_nominal_bytes: String,
    pub outputs: usize,
}
impl FramePool {
    pub fn snapshot(&self) -> PoolSnapshot {
        let state = self.state.borrow();
        PoolSnapshot {
            slots: state.slots,
            nominal_bytes: state.bytes.to_string(),
            orphaned: state.orphaned,
            orphaned_nominal_bytes: state.orphaned_bytes.to_string(),
            outputs: state.outputs.len(),
        }
    }
    /// Reserve before allocation. The callback must return only after all GPU
    /// rendering/acquire work has completed. Bytes account nominal payload only.
    pub fn prepare<R, S, E>(
        &self,
        bytes: u64,
        allocate: impl FnOnce() -> Result<(R, S), E>,
    ) -> Result<PreparedFrame<R, S>, PreparationError<E>> {
        {
            let mut state = self.state.borrow_mut();
            if bytes == 0
                || bytes > MAX_FRAME_BYTES
                || state.slots == MAX_FRAME_SLOTS
                || state.bytes > MAX_FRAME_BYTES - bytes
            {
                return Err(PreparationError::Admission(Error::Capacity));
            }
            state.slots += 1;
            state.bytes += bytes;
        }
        let permit = Permit {
            pool: self.clone(),
            bytes,
        };
        let (resource, scene) = allocate().map_err(PreparationError::Backend)?;
        Ok(PreparedFrame {
            payload: Some(Payload {
                resource,
                scene,
                permit,
            }),
            consuming: false,
        })
    }
    pub fn output<R, S>(&self, key: OutputKey) -> Result<FrameQueue<R, S>, Error> {
        {
            let mut state = self.state.borrow_mut();
            if state.outputs.contains(&key) {
                return Err(Error::DuplicateOutput);
            }
            if state.outputs.len() == MAX_OUTPUTS {
                return Err(Error::Capacity);
            }
            state.outputs.insert(key);
        }
        Ok(FrameQueue {
            key,
            front: None,
            pending: None,
            active: true,
            faulted: false,
            closed: false,
            _output: OutputPermit {
                pool: self.clone(),
                key,
            },
        })
    }
}
#[derive(Debug, PartialEq, Eq)]
pub enum PreparationError<E> {
    Admission(Error),
    Backend(E),
}
struct Permit {
    pool: FramePool,
    bytes: u64,
}
impl Drop for Permit {
    fn drop(&mut self) {
        let mut state = self.pool.state.borrow_mut();
        state.slots -= 1;
        state.bytes -= self.bytes;
    }
}
struct OutputPermit {
    pool: FramePool,
    key: OutputKey,
}
impl Drop for OutputPermit {
    fn drop(&mut self) {
        self.pool.state.borrow_mut().outputs.remove(&self.key);
    }
}
struct Payload<R, S> {
    resource: R,
    scene: S,
    permit: Permit,
}
/// Prepared resources cannot be cloned/extracted. Dropping a submitted/front
/// frame without completion leaks its payload AND quota permit deliberately;
/// it never manufactures consumer completion. One pool must own a backend's
/// whole lifetime so those fail-closed reservations cannot be bypassed by reset.
pub struct PreparedFrame<R, S> {
    payload: Option<Payload<R, S>>,
    consuming: bool,
}
impl<R, S> Drop for PreparedFrame<R, S> {
    fn drop(&mut self) {
        if self.consuming
            && let Some(payload) = self.payload.take()
        {
            {
                let mut state = payload.permit.pool.state.borrow_mut();
                state.orphaned += 1;
                state.orphaned_bytes += payload.permit.bytes;
            }
            std::mem::forget(payload);
        }
    }
}
/// All resources remain owned on uncertain submission. A definite rejection
/// requires a platform guarantee that KMS never accepted any consumer.
pub enum SubmitOutcome<E> {
    Accepted,
    Rejected(E),
    Uncertain(E),
}
pub enum SubmitError<R, S, E> {
    Admission {
        reason: Error,
        frame: PreparedFrame<R, S>,
    },
    Rejected {
        error: E,
        frame: PreparedFrame<R, S>,
    },
    Uncertain(E),
}
struct Pending<R, S> {
    cookie: Cookie,
    frame: PreparedFrame<R, S>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Completion {
    Ignored,
    Presented,
    RetiredProgress,
}
#[derive(Debug, Serialize)]
pub struct QueueSnapshot {
    pub key: OutputKey,
    pub active: bool,
    pub faulted: bool,
    pub closed: bool,
    pub front_cookie: Option<Cookie>,
    pub pending_cookie: Option<Cookie>,
}
struct Front<R, S> {
    cookie: Cookie,
    frame: PreparedFrame<R, S>,
}
pub struct FrameQueue<R, S> {
    key: OutputKey,
    front: Option<Front<R, S>>,
    pending: Option<Pending<R, S>>,
    active: bool,
    faulted: bool,
    closed: bool,
    _output: OutputPermit,
}
/// This witness is an unsafe backend boundary, never an IPC acknowledgement.
pub struct IdleProof {
    key: OutputKey,
}
impl IdleProof {
    /// # Safety
    /// Every scanout consumer for this exact output epoch, including every
    /// accepted/uncertain queued frame, must have conclusively stopped. Pause,
    /// timeout, ioctl rejection, FD-number reuse and hotplug alone are not proof.
    pub unsafe fn after_backend_quiescence(key: OutputKey) -> Self {
        Self { key }
    }
}
impl<R, S> FrameQueue<R, S> {
    pub fn snapshot(&self) -> QueueSnapshot {
        QueueSnapshot {
            key: self.key,
            active: self.active,
            faulted: self.faulted,
            closed: self.closed,
            front_cookie: self.front.as_ref().map(|f| f.cookie),
            pending_cookie: self.pending.as_ref().map(|p| p.cookie),
        }
    }
    pub fn visible_scene(&self) -> Option<&S> {
        if !self.active || self.faulted || self.closed {
            return None;
        }
        self.front
            .as_ref()
            .and_then(|f| f.frame.payload.as_ref().map(|p| &p.scene))
    }
    pub fn submit<E>(
        &mut self,
        frame: PreparedFrame<R, S>,
        submit: impl FnOnce(Cookie, &R) -> SubmitOutcome<E>,
    ) -> Result<Cookie, SubmitError<R, S, E>> {
        self.submit_using(&LAST_COOKIE, frame, submit)
    }
    fn submit_using<E>(
        &mut self,
        counter: &AtomicU64,
        mut frame: PreparedFrame<R, S>,
        submit: impl FnOnce(Cookie, &R) -> SubmitOutcome<E>,
    ) -> Result<Cookie, SubmitError<R, S, E>> {
        let reason = if self.closed || !self.active || self.faulted {
            Some(Error::Inactive)
        } else if self.pending.is_some() {
            Some(Error::Pending)
        } else if !Rc::ptr_eq(
            &frame
                .payload
                .as_ref()
                .expect("prepared payload")
                .permit
                .pool
                .state,
            &self._output.pool.state,
        ) {
            Some(Error::WrongPool)
        } else {
            None
        };
        if let Some(reason) = reason {
            return Err(SubmitError::Admission { reason, frame });
        }
        let cookie = match allocate_cookie(counter) {
            Ok(cookie) => cookie,
            Err(reason) => {
                self.faulted = true;
                return Err(SubmitError::Admission { reason, frame });
            }
        };
        // Own it as uncertain BEFORE invoking platform code. An unwind cannot
        // drop a possibly submitted buffer or leave the old scene authoritative.
        frame.consuming = true;
        self.pending = Some(Pending { cookie, frame });
        self.faulted = true;
        let resource = &self
            .pending
            .as_ref()
            .expect("submitting frame owned")
            .frame
            .payload
            .as_ref()
            .expect("pending payload")
            .resource;
        match submit(cookie, resource) {
            SubmitOutcome::Accepted => {
                self.faulted = false;
                Ok(cookie)
            }
            SubmitOutcome::Rejected(error) => {
                let mut frame = self.pending.take().expect("submitting frame owned").frame;
                frame.consuming = false;
                self.faulted = false;
                Err(SubmitError::Rejected { error, frame })
            }
            SubmitOutcome::Uncertain(error) => Err(SubmitError::Uncertain(error)),
        }
    }
    pub fn complete(&mut self, source: Ticket, event: crate::drm_events::Flip) -> Completion {
        if !self.key.source_matches(source, event.crtc)
            || event.cookie == 0
            || self
                .pending
                .as_ref()
                .is_none_or(|p| p.cookie.get() != event.cookie)
        {
            return Completion::Ignored;
        }
        let pending = self.pending.take().expect("matching pending owned");
        if let Some(mut old) = self.front.take() {
            old.frame.consuming = false;
            drop(old);
        }
        self.front = Some(Front {
            cookie: pending.cookie,
            frame: pending.frame,
        });
        self.faulted = false;
        if self.active && !self.closed {
            Completion::Presented
        } else {
            Completion::RetiredProgress
        }
    }
    /// Stops publication/admission immediately, without pretending consumers idle.
    pub fn invalidate(&mut self) {
        self.active = false;
    }
    /// Missing completion is an uncertainty condition, never permission to free.
    pub fn timeout(&mut self) -> bool {
        if self.pending.is_some() {
            self.faulted = true;
            true
        } else {
            false
        }
    }
    pub fn release_after_disable(&mut self, proof: IdleProof) -> Result<(), Error> {
        if proof.key != self.key {
            return Err(Error::WrongProof);
        }
        self.active = false;
        self.closed = true;
        self.faulted = false;
        if let Some(front) = &mut self.front {
            front.frame.consuming = false;
        }
        if let Some(pending) = &mut self.pending {
            pending.frame.consuming = false;
        }
        self.front.take();
        self.pending.take();
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[derive(Clone)]
    struct Resource {
        id: u32,
        dropped: Rc<RefCell<Vec<u32>>>,
    }
    impl Drop for Resource {
        fn drop(&mut self) {
            self.dropped.borrow_mut().push(self.id);
        }
    }
    fn ticket() -> Ticket {
        Ticket {
            device: 1,
            generation: 2,
            session_epoch: 3,
        }
    }
    fn key() -> OutputKey {
        OutputKey::new(ticket(), 7, 4).unwrap()
    }
    fn prepared(
        pool: &FramePool,
        id: u32,
        dropped: &Rc<RefCell<Vec<u32>>>,
    ) -> PreparedFrame<Resource, String> {
        pool.prepare(4, || {
            Ok::<_, ()>((
                Resource {
                    id,
                    dropped: dropped.clone(),
                },
                format!("scene-{id}"),
            ))
        })
        .unwrap()
    }
    fn event(cookie: Cookie) -> crate::drm_events::Flip {
        crate::drm_events::Flip {
            cookie: cookie.get(),
            crtc: 7,
            sequence: 0,
            seconds: 1,
            microseconds: 0,
        }
    }
    fn submit(
        queue: &mut FrameQueue<Resource, String>,
        frame: PreparedFrame<Resource, String>,
    ) -> Cookie {
        queue
            .submit::<()>(frame, |_, resource| {
                assert!(resource.id > 0);
                SubmitOutcome::Accepted
            })
            .unwrap_or_else(|_| panic!("submission rejected"))
    }
    fn disable<R, S>(queue: &mut FrameQueue<R, S>) {
        // SAFETY: these tests have only fake consumers; they are now idle.
        let proof = unsafe { IdleProof::after_backend_quiescence(queue.key) };
        queue.release_after_disable(proof).unwrap();
    }
    #[test]
    fn front_and_scene_advance_only_on_exact_cookie_then_retire_on_replacement() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let first = submit(&mut queue, prepared(&pool, 1, &dropped));
        assert!(queue.visible_scene().is_none());
        assert!(dropped.borrow().is_empty());
        let mut wrong = event(first);
        wrong.cookie = 0;
        assert_eq!(queue.complete(ticket(), wrong), Completion::Ignored);
        wrong = event(first);
        wrong.cookie += 1000;
        assert_eq!(queue.complete(ticket(), wrong), Completion::Ignored);
        assert_eq!(
            queue.complete(
                Ticket {
                    session_epoch: 4,
                    ..ticket()
                },
                event(first)
            ),
            Completion::Ignored
        );
        wrong = event(first);
        wrong.crtc = 8;
        assert_eq!(queue.complete(ticket(), wrong), Completion::Ignored);
        assert_eq!(
            queue.complete(ticket(), event(first)),
            Completion::Presented
        );
        assert_eq!(queue.visible_scene().unwrap(), "scene-1");
        assert!(dropped.borrow().is_empty());
        let next = submit(&mut queue, prepared(&pool, 2, &dropped));
        assert_eq!(queue.visible_scene().unwrap(), "scene-1");
        assert_eq!(queue.complete(ticket(), event(first)), Completion::Ignored);
        assert!(dropped.borrow().is_empty());
        assert_eq!(queue.complete(ticket(), event(next)), Completion::Presented);
        assert_eq!(*dropped.borrow(), [1]);
        assert_eq!(pool.snapshot().slots, 1);
        assert_eq!(queue.visible_scene().unwrap(), "scene-2");
        disable(&mut queue);
        assert_eq!(*dropped.borrow(), [1, 2]);
        assert_eq!(pool.snapshot().slots, 0);
    }
    #[test]
    fn definite_rejection_returns_ready_resource_without_replacing_front() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let first = submit(&mut queue, prepared(&pool, 1, &dropped));
        queue.complete(ticket(), event(first));
        let result = queue.submit(prepared(&pool, 2, &dropped), |_, _| {
            SubmitOutcome::Rejected("not accepted")
        });
        match result {
            Err(SubmitError::Rejected { error, frame }) => {
                assert_eq!(error, "not accepted");
                drop(frame);
            }
            _ => panic!("expected definite reject"),
        }
        assert_eq!(*dropped.borrow(), [2]);
        assert_eq!(queue.visible_scene().unwrap(), "scene-1");
        assert!(queue.snapshot().pending_cookie.is_none());
        disable(&mut queue);
    }
    #[test]
    fn uncertain_submit_timeout_and_invalidation_never_free_inflight_frames() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let first = submit(&mut queue, prepared(&pool, 1, &dropped));
        queue.complete(ticket(), event(first));
        assert!(!queue.timeout());
        assert_eq!(queue.visible_scene().unwrap(), "scene-1");
        assert!(matches!(
            queue.submit(prepared(&pool, 2, &dropped), |_, _| {
                SubmitOutcome::Uncertain("unknown")
            }),
            Err(SubmitError::Uncertain("unknown"))
        ));
        let next = queue.snapshot().pending_cookie.unwrap();
        assert!(queue.timeout());
        assert!(queue.visible_scene().is_none());
        assert!(dropped.borrow().is_empty());
        queue.invalidate();
        assert_eq!(
            queue.complete(ticket(), event(next)),
            Completion::RetiredProgress
        );
        assert_eq!(*dropped.borrow(), [1]);
        assert!(queue.visible_scene().is_none());
        assert_eq!(pool.snapshot().slots, 1);
        disable(&mut queue);
        assert_eq!(*dropped.borrow(), [1, 2]);
    }
    #[test]
    fn backend_unwind_preserves_uncertain_candidate_and_hides_old_scene() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _: Result<_, SubmitError<_, _, ()>> = queue
                .submit(prepared(&pool, 1, &dropped), |_, _| {
                    panic!("uncertain platform unwind")
                });
        }));
        assert!(result.is_err());
        assert!(queue.snapshot().faulted);
        assert!(queue.snapshot().pending_cookie.is_some());
        assert!(dropped.borrow().is_empty());
        disable(&mut queue);
    }
    #[test]
    fn unexpected_drop_orphans_consumer_and_quota_prevents_unbounded_retry() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let frame = pool
            .prepare(MAX_FRAME_BYTES, || {
                Ok::<_, ()>((
                    Resource {
                        id: 1,
                        dropped: dropped.clone(),
                    },
                    "scene".to_string(),
                ))
            })
            .unwrap();
        submit(&mut queue, frame);
        drop(queue);
        assert!(dropped.borrow().is_empty());
        assert_eq!(pool.snapshot().orphaned, 1);
        assert_eq!(pool.snapshot().slots, 1);
        assert_eq!(
            pool.snapshot().orphaned_nominal_bytes,
            MAX_FRAME_BYTES.to_string()
        );
        let invoked = std::cell::Cell::new(false);
        let result = pool.clone().prepare(1, || {
            invoked.set(true);
            Ok::<_, ()>(((), ()))
        });
        assert!(matches!(
            result,
            Err(PreparationError::Admission(Error::Capacity))
        ));
        assert!(!invoked.get());
    }
    #[test]
    fn admission_precedes_allocation_and_wrong_proof_cannot_release() {
        let pool = FramePool::default();
        assert!(matches!(
            pool.prepare(1, || Err::<((), ()), _>(7)),
            Err(PreparationError::Backend(7))
        ));
        assert_eq!(pool.snapshot().slots, 0);
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        assert!(matches!(
            pool.output::<(), ()>(key()),
            Err(Error::DuplicateOutput)
        ));
        let other = FramePool::default();
        let called = std::cell::Cell::new(false);
        let result = queue.submit::<()>(prepared(&other, 1, &dropped), |_, _| {
            called.set(true);
            SubmitOutcome::Accepted
        });
        assert!(matches!(
            result,
            Err(SubmitError::Admission {
                reason: Error::WrongPool,
                ..
            })
        ));
        drop(result);
        assert!(!called.get());
        let first = submit(&mut queue, prepared(&pool, 2, &dropped));
        let result = queue.submit::<()>(prepared(&pool, 3, &dropped), |_, _| {
            called.set(true);
            SubmitOutcome::Accepted
        });
        assert!(matches!(
            result,
            Err(SubmitError::Admission {
                reason: Error::Pending,
                ..
            })
        ));
        drop(result);
        assert!(!called.get());
        let wrong = OutputKey::new(ticket(), 8, 4).unwrap();
        // SAFETY: this proves only the other fake output idle, not this queue.
        assert_eq!(
            queue.release_after_disable(unsafe { IdleProof::after_backend_quiescence(wrong) }),
            Err(Error::WrongProof)
        );
        assert_eq!(queue.snapshot().pending_cookie, Some(first));
        disable(&mut queue);
    }
    #[test]
    fn cookie_counter_never_wraps_and_exhaustion_seals_admission() {
        let counter = AtomicU64::new(u64::MAX - 1);
        assert_eq!(allocate_cookie(&counter).unwrap().get(), u64::MAX);
        assert_eq!(allocate_cookie(&counter), Err(Error::Exhausted));
        assert_eq!(counter.load(Ordering::Relaxed), u64::MAX);
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut queue = pool.output(key()).unwrap();
        let result = queue.submit_using::<()>(&counter, prepared(&pool, 1, &dropped), |_, _| {
            panic!("exhaustion must precede submit")
        });
        assert!(matches!(
            result,
            Err(SubmitError::Admission {
                reason: Error::Exhausted,
                ..
            })
        ));
        drop(result);
        assert!(queue.snapshot().faulted);
        disable(&mut queue);
    }
    #[test]
    fn old_output_epoch_cookie_cannot_complete_reused_crtc() {
        let pool = FramePool::default();
        let dropped: Rc<RefCell<Vec<u32>>> = Rc::default();
        let mut old = pool.output(key()).unwrap();
        let previous = submit(&mut old, prepared(&pool, 1, &dropped));
        disable(&mut old);
        drop(old);
        let mut next = pool
            .output(OutputKey::new(ticket(), 7, 5).unwrap())
            .unwrap();
        let current = submit(&mut next, prepared(&pool, 2, &dropped));
        assert_ne!(previous, current);
        assert_eq!(
            next.complete(ticket(), event(previous)),
            Completion::Ignored
        );
        assert_eq!(
            next.complete(ticket(), event(current)),
            Completion::Presented
        );
        disable(&mut next);
    }
}
