//! Renderer-side Clipboard request forwarding and Promise settlement.

use super::super::{AdvanceResult, DocumentRuntime};
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::{ClipboardRequest, ClipboardUpdate};
use std::time::Instant;

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn start_pending_clipboard_requests(
        &mut self,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for action in std::mem::take(&mut self.pending_clipboard_requests) {
            connection.send_clipboard_request(ClipboardRequest {
                document: self.id,
                request_id: action.request_id,
                client: action.client,
                action: action.action,
            })?;
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_clipboard_update(
        &mut self,
        update: ClipboardUpdate,
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
            .map(|runtime| runtime.deliver_clipboard_update(update))
            .unwrap_or_default();
        self.complete_network_script_outcome(
            outcome,
            true,
            previous_timer_micros,
            started,
            connection,
        )
    }
}
