//! Bound ordinary toplevel configures without replacing Smithay's serial rules.
//!
//! One unacknowledged configure owns the wire slot. Smithay's server_pending
//! retains the latest desired state while it is occupied. An accepted ack frees
//! the slot; it does not prove that the client committed or rendered that state.
use smithay::{
    utils::Serial,
    wayland::{
        compositor::with_states,
        shell::xdg::{ToplevelCachedState, ToplevelSurface, XdgToplevelSurfaceData},
    },
};

pub fn send_pending(surface: &ToplevelSurface) -> Option<Serial> {
    let available = with_states(surface.wl_surface(), |states| {
        let data = states.data_map.get::<XdgToplevelSurfaceData>().unwrap();
        let data = data.lock().unwrap();
        data.initial_configure_sent && data.pending_configures().is_empty()
    });
    // Do not hold the role mutex while Smithay takes it to send the event.
    available
        .then(|| surface.send_pending_configure())
        .flatten()
}

#[derive(serde::Serialize)]
pub struct Snapshot {
    pub unacknowledged: usize,
    pub deferred: bool,
    pub initial_sent: bool,
    pub last_acked_serial: Option<u32>,
    pub committed_serial: Option<u32>,
    pub desired_size: Option<(i32, i32)>,
    pub sent_size: Option<(i32, i32)>,
    pub committed_size: Option<(i32, i32)>,
}

pub fn snapshot(surface: &ToplevelSurface) -> Snapshot {
    with_states(surface.wl_surface(), |states| {
        let data = states.data_map.get::<XdgToplevelSurfaceData>().unwrap();
        let data = data.lock().unwrap();
        let sent = data.current_server_state();
        let desired = data.server_pending.as_ref().unwrap_or(&sent);
        let mut cached = states.cached_state.get::<ToplevelCachedState>();
        let committed = cached.current().last_acked.as_ref();
        Snapshot {
            unacknowledged: data.pending_configures().len(),
            deferred: data.has_pending_changes(),
            initial_sent: data.initial_configure_sent,
            last_acked_serial: data.last_acked.as_ref().map(|c| c.serial.into()),
            committed_serial: committed.map(|c| c.serial.into()),
            desired_size: desired.size.map(Into::into),
            sent_size: sent.size.map(Into::into),
            committed_size: committed.and_then(|c| c.state.size.map(Into::into)),
        }
    })
}
