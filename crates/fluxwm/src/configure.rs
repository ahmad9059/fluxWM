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

use std::sync::atomic::{AtomicBool, Ordering};
#[derive(Default)]
struct StateReply(AtomicBool);

/// At most one required reply is retained while a configure owns the wire slot.
pub fn defer_reply(surface: &ToplevelSurface) {
    with_states(surface.wl_surface(), |states| {
        states.data_map.insert_if_missing(StateReply::default);
        states
            .data_map
            .get::<StateReply>()
            .unwrap()
            .0
            .store(true, Ordering::Relaxed);
    });
}
pub fn clear_reply(surface: &ToplevelSurface) {
    with_states(surface.wl_surface(), |states| {
        if let Some(reply) = states.data_map.get::<StateReply>() {
            reply.0.store(false, Ordering::Relaxed);
        }
    });
}
pub fn send_pending(surface: &ToplevelSurface) -> Option<Serial> {
    let (available, forced) = with_states(surface.wl_surface(), |states| {
        let data = states.data_map.get::<XdgToplevelSurfaceData>().unwrap();
        let data = data.lock().unwrap();
        let available = data.initial_configure_sent && data.pending_configures().is_empty();
        let forced = available
            && states
                .data_map
                .get::<StateReply>()
                .is_some_and(|reply| reply.0.swap(false, Ordering::Relaxed));
        (available, forced)
    });
    // Drop the role mutex before sending. Repeated state requests require a
    // configure even when no desired fields changed; admission remains bounded.
    if forced {
        Some(surface.send_configure())
    } else if available {
        surface.send_pending_configure()
    } else {
        None
    }
}

/// Validate the state attached to this commit, not a newer unacknowledged target.
pub fn valid_maximized_size(
    surface: &ToplevelSurface,
    size: smithay::utils::Size<i32, smithay::utils::Logical>,
) -> bool {
    with_states(surface.wl_surface(), |states| {
        let mut cached = states.cached_state.get::<ToplevelCachedState>();
        let Some(committed) = cached.current().last_acked.as_ref() else {
            return true;
        };
        !committed.state.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized)
            || committed.state.size.is_none_or(|expected| expected == size)
    })
}

#[derive(serde::Serialize)]
pub struct Snapshot {
    pub unacknowledged: usize,
    pub deferred: bool,
    pub request_reply_deferred: bool,
    pub desired_maximized: bool,
    pub sent_maximized: bool,
    pub committed_maximized: bool,
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
            request_reply_deferred: states.data_map.get::<StateReply>().is_some_and(|r| r.0.load(Ordering::Relaxed)),
            desired_maximized: desired.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized),
            sent_maximized: sent.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized),
            committed_maximized: committed.is_some_and(|c| c.state.states.contains(smithay::reexports::wayland_protocols::xdg::shell::server::xdg_toplevel::State::Maximized)),
            initial_sent: data.initial_configure_sent,
            last_acked_serial: data.last_acked.as_ref().map(|c| c.serial.into()),
            committed_serial: committed.map(|c| c.serial.into()),
            desired_size: desired.size.map(Into::into),
            sent_size: sent.size.map(Into::into),
            committed_size: committed.and_then(|c| c.state.size.map(Into::into)),
        }
    })
}
