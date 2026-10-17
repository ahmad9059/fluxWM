// Adapted from Smithay Smallvil, MIT; see LICENSES/Smithay-MIT.txt.
use crate::{Fluxwm, grabs::resize_grab, state::ClientState};
use smithay::{
    backend::renderer::utils::on_commit_buffer_handler,
    reexports::wayland_server::{
        Client,
        protocol::{wl_buffer, wl_surface::WlSurface},
    },
    wayland::{
        buffer::BufferHandler,
        compositor::{
            CompositorClientState, CompositorHandler, CompositorState, get_parent,
            is_sync_subsurface,
        },
        shm::{ShmHandler, ShmState},
    },
};

use super::xdg_shell;

impl CompositorHandler for Fluxwm {
    fn compositor_state(&mut self) -> &mut CompositorState {
        &mut self.compositor_state
    }

    fn client_compositor_state<'a>(&self, client: &'a Client) -> &'a CompositorClientState {
        &client.get_data::<ClientState>().unwrap().compositor_state
    }

    fn commit(&mut self, surface: &WlSurface) {
        on_commit_buffer_handler::<Self>(surface);
        let mut just_unmapped = false;
        if !is_sync_subsurface(surface) {
            let mut root = surface.clone();
            while let Some(parent) = get_parent(&root) {
                root = parent;
            }
            let id = self.window_id(&root);
            let was_buffered = id
                .and_then(|id| self.windows.get(id))
                .is_some_and(|item| item.buffered);
            self.commit_window(&root);
            just_unmapped = was_buffered
                && !id
                    .and_then(|id| self.windows.get(id))
                    .is_some_and(|item| item.buffered);
        };

        xdg_shell::handle_commit(self, surface, just_unmapped);
        self.request_redraw();
        resize_grab::handle_commit(&mut self.space, surface);
    }
}

impl BufferHandler for Fluxwm {
    fn buffer_destroyed(&mut self, _buffer: &wl_buffer::WlBuffer) {}
}

impl ShmHandler for Fluxwm {
    fn shm_state(&self) -> &ShmState {
        &self.shm_state
    }
}
