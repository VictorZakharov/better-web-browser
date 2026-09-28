//! Renderer-side Geolocation command forwarding and callback delivery.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::{GeolocationRequest, GeolocationUpdate};
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_geolocation_requests(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for action in std::mem::take(&mut self.pending_geolocation_requests) {
            connection.send_geolocation_request(GeolocationRequest {
                document: self.id,
                request_id: action.request_id,
                client: action.client,
                action: action.action,
            })?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_geolocation_update(
        &mut self,
        update: GeolocationUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document != self.id {
            return Ok(None);
        }
        let terminal = update.terminal;
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_geolocation_update(update))
            .unwrap_or_default();
        self.complete_network_script_outcome(
            outcome,
            terminal,
            previous_timer_micros,
            started,
            connection,
        )
    }
}
