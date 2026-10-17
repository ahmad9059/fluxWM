use crate::Fluxwm;
use serde_json::json;
use std::sync::Arc;

impl Fluxwm {
    pub fn init_autostart(
        &mut self,
        handle: &calloop::LoopHandle<'static, Self>,
    ) -> std::io::Result<()> {
        let (ping, source) = calloop::ping::make_ping()?;
        handle
            .insert_source(source, |(), _, state: &mut Self| {
                state.autostart_notified = true
            })
            .map_err(|e| std::io::Error::other(e.to_string()))?;
        self.autostart = Some(flux_process::AutostartWorker::new(move || ping.ping())?);
        self.autostart_enabled = true;
        Ok(())
    }
    pub fn schedule_autostart(&mut self, plan: Arc<flux_process::AutostartPlan>) {
        self.autostart_generation = Some(plan.generation());
        self.autostart_pending = Some(plan);
        self.autostart_notified = true;
    }
    pub fn drive_autostart(&mut self) {
        if !std::mem::take(&mut self.autostart_notified) {
            return;
        }
        let mut changed = false;
        if let Some(worker) = self.autostart.as_ref() {
            if let Some(snapshot) = worker.take_snapshot() {
                self.autostart_view = snapshot;
                changed = true;
            }
            if !self.stopping
                && self.shutdown_deadline.is_none()
                && let Some(plan) = self.autostart_pending.as_ref()
            {
                match worker.try_install(plan) {
                    Ok(()) => {
                        self.autostart_pending = None;
                    }
                    Err(flux_process::Error::QueueFull) => (),
                    Err(_) => {
                        self.autostart_view.error = Some("autostart plan could not be admitted");
                        self.autostart_view.requested_generation = self.autostart_generation;
                        self.autostart_view.settled = true;
                        self.autostart_pending = None;
                        changed = true;
                    }
                }
            }
        }
        if changed {
            self.bump_sequence();
        }
    }
    pub fn autostart_diagnostics(&self) -> serde_json::Value {
        let v = &self.autostart_view;
        json!({
            "enabled":self.autostart_enabled,"requested_generation":self.autostart_generation.map(|g|g.to_string()),
            "observed_generation":v.requested_generation.map(|g|g.to_string()),"active_generation":v.active_generation.map(|g|g.to_string()),
            "active_complete":v.active_complete,"retained_argv_bytes":v.retained_argv_bytes,
            "settled":!self.autostart_enabled || (v.settled && (v.closed || v.requested_generation==self.autostart_generation)),
            "closed":v.closed,"error":v.error,
            "entries":v.entries.iter().map(|s|json!({"slot":s.slot,"id":s.id,"index":s.index,"desired":s.desired,"attempts":s.attempts.to_string(),"starts":s.starts.to_string(),"pid":s.pid,"state":s.state,"change_deferred":s.change_deferred,"error":s.error,"errno":s.errno,"exit_code":s.exit_code,"exit_signal":s.exit_signal})).collect::<Vec<_>>(),
        })
    }
    pub fn shutdown_autostart(&mut self) -> std::io::Result<()> {
        if let Some(mut worker) = self.autostart.take() {
            worker.shutdown()?;
            if let Some(snapshot) = worker.take_snapshot() {
                self.autostart_view = snapshot;
            }
            self.stats.autostart = Some(self.autostart_diagnostics());
        }
        Ok(())
    }
}
