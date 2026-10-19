//! Private nonblocking DRM reader preserving the full kernel user_data cookie.
use crate::drm_events::{self, Flip};
use smithay::reexports::calloop::{
    EventSource, Interest, Mode, Poll, PostAction, Readiness, Token, TokenFactory, generic::Generic,
};
use std::{
    cell::Cell,
    io,
    os::fd::{AsRawFd, BorrowedFd, OwnedFd},
    rc::Rc,
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Read(Option<i32>),
    Closed,
    Malformed(drm_events::DecodeError),
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Event {
    pub generation: u64,
    pub event: Result<Flip, Failure>,
}
pub(super) struct Source {
    fd: Generic<OwnedFd>,
    failed: bool,
    generation: Rc<Cell<u64>>,
}
impl Source {
    pub fn new(fd: OwnedFd, generation: Rc<Cell<u64>>) -> io::Result<Self> {
        super::enforce_flags(&fd)?;
        Ok(Self {
            fd: Generic::new(fd, Interest::READ, Mode::Level),
            failed: false,
            generation,
        })
    }
}
impl EventSource for Source {
    type Event = Event;
    type Metadata = ();
    type Ret = ();
    type Error = io::Error;
    fn process_events<F>(
        &mut self,
        r: Readiness,
        t: Token,
        mut callback: F,
    ) -> io::Result<PostAction>
    where
        F: FnMut(Self::Event, &mut ()),
    {
        let failed = &mut self.failed;
        let current_generation = &self.generation;
        self.fd.process_events(r, t, |_, fd| {
            if *failed {
                return Ok(PostAction::Disable);
            }
            // A callback may advance the generation while processing this
            // batch. Every event already read must retain its original stamp.
            let generation = current_generation.get();
            let mut bytes = [0u8; drm_events::MAX_EVENT_BYTES];
            // SAFETY: the borrowed source FD and writable stack buffer are live
            // for this call. One bounded nonblocking read per readiness dispatch.
            let count =
                unsafe { libc::read(fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
            let result = if count < 0 {
                let error = io::Error::last_os_error();
                if matches!(error.raw_os_error(), Some(libc::EAGAIN) | Some(libc::EINTR)) {
                    return Ok(PostAction::Continue);
                }
                Err(Failure::Read(error.raw_os_error()))
            } else if count == 0 {
                Err(Failure::Closed)
            } else {
                drm_events::decode(&bytes[..count as usize]).map_err(Failure::Malformed)
            };
            match result {
                Ok(events) => {
                    for event in events {
                        callback(
                            Event {
                                generation,
                                event: Ok(event),
                            },
                            &mut (),
                        );
                    }
                    Ok(PostAction::Continue)
                }
                Err(error) => {
                    *failed = true;
                    callback(
                        Event {
                            generation,
                            event: Err(error),
                        },
                        &mut (),
                    );
                    Ok(PostAction::Disable)
                }
            }
        })
    }
    fn register(
        &mut self,
        p: &mut Poll,
        f: &mut TokenFactory,
    ) -> smithay::reexports::calloop::Result<()> {
        self.fd.register(p, f)
    }
    fn reregister(
        &mut self,
        p: &mut Poll,
        f: &mut TokenFactory,
    ) -> smithay::reexports::calloop::Result<()> {
        self.fd.reregister(p, f)
    }
    fn unregister(&mut self, p: &mut Poll) -> smithay::reexports::calloop::Result<()> {
        self.fd.unregister(p)
    }
}
/// Discard queued bytes after a synchronous commit has quiesced the selected
/// CRTC, before submitting any cookie-bearing frame. The caller must serialize
/// source dispatch and new commits through this barrier. No decoding is needed.
/// A saturated budget fails closed; it does not prove the stream is empty.
pub(super) fn drain(fd: BorrowedFd<'_>) -> io::Result<()> {
    // SAFETY: F_GETFL only reads flags of this live borrowed descriptor.
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
    if flags < 0 {
        return Err(io::Error::last_os_error());
    }
    if flags & libc::O_NONBLOCK == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "DRM drain requires nonblocking fd",
        ));
    }
    let mut bytes = [0u8; drm_events::MAX_EVENT_BYTES];
    for _ in 0..64 {
        // SAFETY: fd and writable stack bytes remain live through the read.
        let count = unsafe { libc::read(fd.as_raw_fd(), bytes.as_mut_ptr().cast(), bytes.len()) };
        if count > 0 {
            continue;
        }
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "DRM event stream closed during drain",
            ));
        }
        let error = io::Error::last_os_error();
        match error.raw_os_error() {
            Some(libc::EAGAIN) => return Ok(()),
            Some(libc::EINTR) => continue,
            _ => return Err(error),
        }
    }
    Err(io::Error::new(
        io::ErrorKind::WouldBlock,
        "DRM event drain read budget exhausted",
    ))
}
#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::calloop::EventLoop;
    use std::{io::Write, os::unix::net::UnixStream, time::Duration};
    #[test]
    fn batch_stamp_stays_old_after_callback_advances_generation() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        let generation = Rc::new(Cell::new(1));
        let callback_generation = generation.clone();
        let mut event_loop = EventLoop::<(Vec<Event>, Vec<u64>)>::try_new().unwrap();
        event_loop
            .handle()
            .insert_source(
                Source::new(reader.into(), generation.clone()).unwrap(),
                move |event, _, (all, accepted)| {
                    all.push(event);
                    if event.generation == callback_generation.get() {
                        let cookie = event.event.unwrap().cookie;
                        accepted.push(cookie);
                        if cookie == 11 {
                            callback_generation.set(2);
                        }
                    }
                },
            )
            .unwrap();
        let batch = [
            crate::drm_events::encoded(11, 7, 1),
            crate::drm_events::encoded(12, 7, 2),
        ]
        .concat();
        writer.write_all(&batch).unwrap();
        let mut state = (vec![], vec![]);
        event_loop
            .dispatch(Duration::from_millis(100), &mut state)
            .unwrap();
        assert_eq!(
            state
                .0
                .iter()
                .map(|event| event.generation)
                .collect::<Vec<_>>(),
            [1, 1]
        );
        assert_eq!(state.1, [11]);
        writer
            .write_all(&crate::drm_events::encoded(13, 7, 3))
            .unwrap();
        event_loop
            .dispatch(Duration::from_millis(100), &mut state)
            .unwrap();
        assert_eq!(state.0[2].generation, 2);
        assert_eq!(state.1, [11, 13]);
    }
    #[test]
    fn drain_discards_queued_packets_until_eagain_and_rejects_eof() {
        use std::os::fd::AsFd;
        let (reader, mut writer) = UnixStream::pair().unwrap();
        reader.set_nonblocking(true).unwrap();
        writer
            .write_all(
                &[
                    crate::drm_events::encoded(11, 7, 1),
                    crate::drm_events::encoded(12, 7, 2),
                ]
                .concat(),
            )
            .unwrap();
        drain(reader.as_fd()).unwrap();
        drain(reader.as_fd()).unwrap();
        drop(writer);
        assert_eq!(
            drain(reader.as_fd()).unwrap_err().kind(),
            io::ErrorKind::UnexpectedEof
        );
    }
    #[test]
    fn drain_rejects_blocking_descriptor_without_reading() {
        use std::os::fd::AsFd;
        let (reader, _writer) = UnixStream::pair().unwrap();
        assert_eq!(
            drain(reader.as_fd()).unwrap_err().kind(),
            io::ErrorKind::InvalidInput
        );
    }
    #[test]
    fn owned_pipe_dispatch_preserves_cookie_and_disables_after_malformed_read() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        let mut event_loop = EventLoop::<Vec<Event>>::try_new().unwrap();
        let generation = Rc::new(Cell::new(1));
        event_loop
            .handle()
            .insert_source(
                Source::new(reader.into(), generation.clone()).unwrap(),
                |event, _, events| events.push(event),
            )
            .unwrap();
        writer
            .write_all(&crate::drm_events::encoded(u64::MAX, 7, 3))
            .unwrap();
        let mut events = vec![];
        event_loop
            .dispatch(Duration::from_millis(100), &mut events)
            .unwrap();
        assert_eq!(events[0].event.as_ref().unwrap().cookie, u64::MAX);
        assert_eq!(events[0].generation, 1);
        generation.set(7);
        writer.write_all(&[0; 8]).unwrap();
        event_loop
            .dispatch(Duration::from_millis(100), &mut events)
            .unwrap();
        assert!(matches!(events[1].event, Err(Failure::Malformed(_))));
        assert_eq!(events[1].generation, 7);
        writer
            .write_all(&crate::drm_events::encoded(1, 7, 4))
            .unwrap();
        event_loop.dispatch(Duration::ZERO, &mut events).unwrap();
        assert_eq!(events.len(), 2);
    }
}
