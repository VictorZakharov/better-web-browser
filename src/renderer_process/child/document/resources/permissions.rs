//! Browser-authoritative permission queries and state delivery.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::PermissionUpdate;
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_permission_requests(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for mut request in std::mem::take(&mut self.pending_permission_requests) {
            request.document = self.id;
            connection.send_permission_request(request)?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_permission_update(
        &mut self,
        update: PermissionUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document != self.id {
            return Ok(None);
        }
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_permission_update(update))
            .unwrap_or_default();
        self.complete_network_script_outcome(
            outcome,
            false,
            previous_timer_micros,
            started,
            connection,
        )
    }
}
