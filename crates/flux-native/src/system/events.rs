//! Private nonblocking DRM reader preserving the full kernel user_data cookie.
use crate::drm_events::{self, Flip};
use smithay::reexports::calloop::{
    EventSource, Interest, Mode, Poll, PostAction, Readiness, Token, TokenFactory, generic::Generic,
};
use std::{
    io,
    os::fd::{AsRawFd, OwnedFd},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Failure {
    Read(Option<i32>),
    Closed,
    Malformed(drm_events::DecodeError),
}
pub(super) struct Source {
    fd: Generic<OwnedFd>,
    failed: bool,
}
impl Source {
    pub fn new(fd: OwnedFd) -> io::Result<Self> {
        super::enforce_flags(&fd)?;
        Ok(Self {
            fd: Generic::new(fd, Interest::READ, Mode::Level),
            failed: false,
        })
    }
}
impl EventSource for Source {
    type Event = Result<Flip, Failure>;
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
        self.fd.process_events(r, t, |_, fd| {
            if *failed {
                return Ok(PostAction::Disable);
            }
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
                        callback(Ok(event), &mut ());
                    }
                    Ok(PostAction::Continue)
                }
                Err(error) => {
                    *failed = true;
                    callback(Err(error), &mut ());
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
#[cfg(test)]
mod tests {
    use super::*;
    use smithay::reexports::calloop::EventLoop;
    use std::{io::Write, os::unix::net::UnixStream, time::Duration};
    #[test]
    fn owned_pipe_dispatch_preserves_cookie_and_disables_after_malformed_read() {
        let (reader, mut writer) = UnixStream::pair().unwrap();
        let mut event_loop = EventLoop::<Vec<Result<Flip, Failure>>>::try_new().unwrap();
        event_loop
            .handle()
            .insert_source(Source::new(reader.into()).unwrap(), |event, _, events| {
                events.push(event)
            })
            .unwrap();
        writer
            .write_all(&crate::drm_events::encoded(u64::MAX, 7, 3))
            .unwrap();
        let mut events = vec![];
        event_loop
            .dispatch(Duration::from_millis(100), &mut events)
            .unwrap();
        assert_eq!(events[0].as_ref().unwrap().cookie, u64::MAX);
        writer.write_all(&[0; 8]).unwrap();
        event_loop
            .dispatch(Duration::from_millis(100), &mut events)
            .unwrap();
        assert!(matches!(events[1], Err(Failure::Malformed(_))));
        writer
            .write_all(&crate::drm_events::encoded(1, 7, 4))
            .unwrap();
        event_loop.dispatch(Duration::ZERO, &mut events).unwrap();
        assert_eq!(events.len(), 2);
    }
}
