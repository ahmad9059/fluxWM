use crate::Fluxwm;
use flux_core::Action;

impl Fluxwm {
    /// Validate user intent at the live model boundary. Unsupported features fail
    /// explicitly; accepting an action does not promise hardware presentation.
    pub fn apply_action(&mut self, action: Action) -> Result<(), String> {
        if self.shutdown_deadline.is_some() {
            return Err("instance is shutting down".into());
        }
        action.validate().map_err(|e| e.to_string())?;
        let next_sequence = self
            .sequence
            .checked_add(1)
            .ok_or("state sequence exhausted")?;
        let geometry_action = matches!(
            action,
            Action::Focus { .. }
                | Action::Move { .. }
                | Action::Pan { .. }
                | Action::Resize { .. }
                | Action::WorkspaceMode { .. }
        );
        if geometry_action && self.seat.get_keyboard().is_some_and(|k| k.is_grabbed()) {
            return Err("a popup owns keyboard focus".into());
        }
        match action {
            Action::Focus { direction } => {
                self.layout
                    .focus_direction(direction)
                    .map_err(|e| e.to_string())?;
                self.relayout();
                self.restore_keyboard_focus();
            }
            Action::Move { direction } => {
                self.layout
                    .move_focused(direction)
                    .map_err(|e| e.to_string())?;
                self.relayout();
            }
            Action::Pan { dx, dy } => {
                self.layout.pan(dx, dy).map_err(|e| e.to_string())?;
                self.relayout();
            }
            Action::Resize { delta } => {
                self.layout
                    .resize_focused(delta)
                    .map_err(|e| e.to_string())?;
                self.relayout();
            }
            Action::WorkspaceMode { mode } => {
                self.layout.set_mode(mode);
                self.relayout();
            }
            Action::CloseWindow => {
                let top = self
                    .layout
                    .focused()
                    .and_then(|id| self.windows.get(id))
                    .and_then(|item| item.window.toplevel())
                    .ok_or("no focused toplevel")?;
                top.send_close();
            }
            Action::Spawn { argv } => {
                if self.spawned.len() + self.pending_spawns >= 64 {
                    return Err("application ownership limit reached".into());
                }
                let worker = self
                    .spawn_worker
                    .as_ref()
                    .ok_or("application worker not ready")?;
                worker
                    .try_spawn(flux_process::SpawnJob {
                        operation: next_sequence,
                        argv,
                        wayland_display: self.socket_name.clone(),
                        ipc_socket: self.ipc_socket.clone(),
                    })
                    .map_err(|e| e.to_string())?;
                self.pending_spawns += 1;
            }
            Action::Quit => {
                let started = std::time::Instant::now();
                let deadline = started + std::time::Duration::from_millis(250);
                self.shutdown_started = Some(started);
                self.shutdown_deadline = Some(deadline);
                self.stats.ipc_drain_budget_ms =
                    Some(deadline.duration_since(started).as_millis() as u64);
                self.ipc_requested = true;
            }
            Action::Fullscreen { .. }
            | Action::Floating { .. }
            | Action::Overview { .. }
            | Action::ReloadConfig
            | Action::Lock => return Err("action is not implemented in the nested backend".into()),
        }
        self.sequence = next_sequence;
        Ok(())
    }
}
