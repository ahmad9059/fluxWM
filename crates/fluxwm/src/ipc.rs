use crate::Fluxwm;
use flux_core::{OutputId, WorkspaceId};
use flux_ipc::{Dispatch, DrainState, ErrorBody, ErrorCode, Method, Request, Response};
use serde_json::{Value, json};
use std::{path::Path, time::Instant};

impl Fluxwm {
    pub fn init_ipc(
        &mut self,
        handle: &calloop::LoopHandle<'static, Self>,
        runtime: &Path,
        instance: &str,
    ) -> std::io::Result<()> {
        let driver =
            flux_ipc::EventLoopDriver::bind(handle, runtime, instance, |state: &mut Self| {
                state.ipc_requested = true
            })?;
        self.ipc_socket = Some(driver.socket_path().to_owned());
        self.ipc = Some(driver);
        self.ipc_requested = true;
        Ok(())
    }
    pub fn drive_ipc(&mut self) {
        if self.stopping || !self.ipc_requested {
            return;
        }
        self.ipc_requested = false;
        // Move ownership out during dispatch: callbacks can mutate authoritative
        // model state, without aliasing it with the driver field being borrowed.
        let Some(mut driver) = self.ipc.take() else {
            return;
        };
        // A keyboard quit is requested outside this driver's handler. IPC quit
        // instead transitions atomically through Dispatch::Drain below, after its
        // response is queued and before another request can execute.
        if let Some(deadline) = self.shutdown_deadline
            && let Err(error) = driver.begin_drain(deadline)
        {
            self.fail(format!("IPC drain integration: {error}"));
        }
        if let Err(error) = driver.poll_controlled(Instant::now(), self.sequence, |request| {
            let response = self.dispatch_ipc(request);
            match self.shutdown_deadline {
                Some(deadline) => Dispatch::Drain { response, deadline },
                None => Dispatch::Continue(response),
            }
        }) {
            self.fail(format!("IPC event integration: {error}"));
        }
        self.stats.ipc_drain_timed_out = driver.stats().drain_timed_out;
        if driver.drain_state() == DrainState::Drained {
            self.stats.ipc_drain_elapsed_ms = self
                .shutdown_started
                .map(|started| started.elapsed().as_millis() as u64);
            self.stop();
        }
        self.ipc = Some(driver);
    }
    fn dispatch_ipc(&mut self, request: Request) -> Response {
        if self.shutdown_deadline.is_some() {
            self.stats.ipc_dispatches_after_shutdown += 1;
            return Response::error(
                request.id,
                self.sequence,
                ErrorBody::new(ErrorCode::NotReady, "instance is shutting down"),
            );
        }
        let result: Result<Value, ErrorBody> = match request.method {
            Method::Version => Ok(
                json!({"name":"FluxWM", "version":env!("CARGO_PKG_VERSION"), "ipc_version":1, "backend":self.backend}),
            ),
            Method::Capabilities => Ok(json!({
                "backend":self.backend, "subscriptions":false, "config_reload":self.config.is_some(),
                "actions":["focus","focus-window","move","pan","resize","workspace-mode","floating","fullscreen","maximize","minimize","restore-window","close-window","disconnect-client","spawn","reload-config","quit"],
                "secure_lock":false,"capture":false,"drag_and_drop":false,"native_session":self.backend == "native","x11_clients":false,
                "client_responsiveness":"xdg-shell-ping", "unresponsive_clients_killed":false,
                "frame_clock":if self.backend == "native" {"drm-page-flip"} else {"virtual-60hz"}, "host_presentation_observed":false,
            })),
            Method::Windows => {
                let placements = self.layout.placements();
                let windows: Vec<_> = self.windows.iter().map(|(id, item)| {
                    let (title,app_id) = item.window.toplevel().map(|t| {
                        smithay::wayland::compositor::with_states(t.wl_surface(), |states| {
                            let Some(data) = states.data_map.get::<smithay::wayland::shell::xdg::XdgToplevelSurfaceData>() else {return (String::new(),String::new())};
                            let data = data.lock().unwrap();
                            (bounded(data.title.as_deref().unwrap_or("")), bounded(data.app_id.as_deref().unwrap_or("")))
                        })
                    }).unwrap_or_default();
                    json!({"id":id,"workspace":WorkspaceId::new(0,0),"mapped":item.mapped,"minimized":self.layout.is_minimized(id),"floating":item.mapped && self.layout.is_floating(id),"minimize_pending":item.minimize_pending,"fullscreen":if item.mapped {self.layout.is_fullscreen(id)} else {item.initial_fullscreen},"maximized":if item.mapped {self.layout.is_maximized(id)} else {item.initial_maximized},"buffered":item.buffered,"admission_error":item.admission_error,"focused":self.layout.focused()==Some(id),"title":title,"app_id":app_id,"responsiveness":item.window.toplevel().map(|t|self.health.for_client(&t.client())),"configure":item.window.toplevel().map(crate::configure::snapshot),"geometry":placements.iter().find(|p|p.window==id).map(|p|p.rect)})
                }).collect();
                Ok(json!({"windows":windows}))
            }
            Method::Workspaces => Ok(
                json!({"workspaces":[{"id":WorkspaceId::new(0,0),"output":OutputId::new(0,0),"mode":self.layout.mode(),"focused":true}]}),
            ),
            Method::Outputs => {
                let outputs:Vec<_> = self.space.outputs().filter_map(|output|self.space.output_geometry(output).map(|g|json!({"id":OutputId::new(0,0),"name":output.name(),"geometry":{"x":g.loc.x,"y":g.loc.y,"width":g.size.w,"height":g.size.h},"scale":1.0}))).collect();
                Ok(json!({"outputs":outputs}))
            }
            Method::EffectiveConfig => Ok(self.effective_config()),
            Method::Diagnostics => Ok(json!({
                "autostart":self.autostart_diagnostics(),
                "configuration":self.config.as_ref().map(|c|c.diagnostics()).unwrap_or_else(||json!({"requested_generation":null,"applied_generation":null,"prepared_generation":null,"status":"disabled","last_error":null})),
                "keyboard_bindings": {"source":if self.config.as_ref().is_some_and(|c|c.applied()) {"user-config"} else {"development-defaults"}, "entries":self.bindings.len(), "inactive":self.bindings.inactive_indices().len(), "runtime_keymap_changes":false},
                "composed_scene": self.scene.as_ref().map(|scene| {
                    let (surfaces, popup_surfaces) = scene.surface_counts();
                    json!({"surfaces":surfaces, "popup_surfaces":popup_surfaces})
                }),
                "input_state": {
                    "host_focus":self.host_focus,
                    "pressed_keys":self.seat.get_keyboard().map_or(0, |k| k.pressed_keys().len()),
                    "suppressed_keys":self.suppressed_keys.len(),
                    "authorized_buttons":self.pointer_presses.len(),
                    "pointer_grabbed":self.seat.get_pointer().is_some_and(|p|p.is_grabbed()),
                    "keyboard_grabbed":self.seat.get_keyboard().is_some_and(|k|k.is_grabbed()),
                },
                "shell_health":self.health.diagnostics(), "runtime":self.stats,
                "live_roles":self.windows.len(), "mapped_windows":self.layout.len(),
                "owned_applications":self.spawned.len(), "pending_launches":self.pending_spawns,
                "launch_outcomes":self.launch_outcomes, "backend":self.backend, "production_ready":false,
            })),
            Method::Action => request.action().and_then(|action| {
                let spawning = matches!(action, flux_core::Action::Spawn { .. });
                let affected = if let flux_core::Action::DisconnectClient { window } = &action {
                    Some(self.disconnect_affected_windows(*window)
                        .map_err(|error| ErrorBody::new(ErrorCode::NotReady, error))?)
                } else { None };
                self.apply_action(action)
                    .map(|()| {
                        if let Some(affected) = affected {
                            json!({"accepted":true,"scope":"wayland-client-connection",
                                "affected_windows":affected,"other_windows_may_be_lost":true})
                        } else if spawning {
                            json!({"accepted":true,"operation_id":self.sequence.to_string()})
                        } else {
                            json!({"accepted":true})
                        }
                    })
                    .map_err(|error| ErrorBody::new(ErrorCode::NotReady, error))
            }),
            Method::ConfigReload => self.apply_action(flux_core::Action::ReloadConfig).map(|()| json!({"accepted":true,"generation":self.config.as_ref().unwrap().diagnostics()["requested_generation"]})).map_err(|error|ErrorBody::new(ErrorCode::NotReady,error)),
            Method::ConfigCheck => Err(ErrorBody::new(
                ErrorCode::NotReady,
                "use fluxwm-config --check; runtime config.check is not integrated",
            )),
            Method::Unknown => Err(ErrorBody::new(ErrorCode::UnknownMethod, "unknown method")),
        };
        match result {
            Ok(value) => Response::success(request.id, self.sequence, value),
            Err(error) => Response::error(request.id, self.sequence, error),
        }
    }
}
fn bounded(value: &str) -> String {
    let mut end = value.len().min(1024);
    while !value.is_char_boundary(end) {
        end -= 1;
    }
    value[..end].to_owned()
}
