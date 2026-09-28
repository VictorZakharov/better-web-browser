//! BroadcastChannel deliveries are separate event-loop tasks in the target document.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::BroadcastDelivery;
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn deliver_broadcast(
        &mut self,
        delivery: BroadcastDelivery,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if delivery.document != self.id {
            return Ok(None);
        }
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_broadcast(&delivery))
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
