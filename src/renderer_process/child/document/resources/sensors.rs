//! Browser-owned sensor commands and document-scoped readings.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::{SensorEvent, SensorRequest, SensorUpdate};
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_sensor_requests(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for action in std::mem::take(&mut self.pending_sensor_requests) {
            connection.send_sensor_request(SensorRequest {
                document: self.id,
                request_id: action.request_id,
                client: action.client,
                user_activation: action.user_activation,
                action: action.action,
            })?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_sensor_update(
        &mut self,
        update: SensorUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document != self.id {
            return Ok(None);
        }
        let terminal = !matches!(update.event, SensorEvent::Reading(_));
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_sensor_update(update))
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
