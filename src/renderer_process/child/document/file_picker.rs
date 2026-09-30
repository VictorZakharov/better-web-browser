//! File picker request admission and bounded browser reply assembly.

use super::{AdvanceResult, DocumentRuntime};
use crate::engine::ScriptOutcome;
use crate::renderer_process::child::connection::ChildConnection;
use crate::renderer_protocol::{
    DocumentNodeId, FilePickerRequest, FilePickerUpdate, FileSelectionAssembler,
};
use std::time::Instant;

impl DocumentRuntime {
    pub(super) fn start_file_picker_requests(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        for action in std::mem::take(&mut outcome.file_picker_actions) {
            let node =
                DocumentNodeId::new(action.node.to_wire()).map_err(|error| error.to_string())?;
            let request = FilePickerRequest {
                document: self.id,
                request_id: action.request_id,
                node,
                client: action.client,
                multiple: action.multiple,
                accept: action.accept,
            };
            connection.send_file_picker_request(request)?;
            self.file_pickers.insert(
                action.request_id,
                FileSelectionAssembler::new(self.id, action.request_id),
            );
        }
        Ok(())
    }

    pub(in crate::renderer_process::child) fn deliver_file_picker_update(
        &mut self,
        update: FilePickerUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document() != self.id {
            return Ok(None);
        }
        let request_id = update.request_id();
        let Some(assembler) = self.file_pickers.get_mut(&request_id) else {
            // A reply for a canceled or replaced document has no owning picker.
            return Ok(None);
        };
        let selection = match assembler.push(update) {
            Ok(selection) => selection,
            Err(error) => {
                self.file_pickers.remove(&request_id);
                return Err(error.to_string());
            }
        };
        let Some(selection) = selection else {
            return Ok(None);
        };
        self.file_pickers.remove(&request_id);
        let previous_timer_micros = self.next_timer_micros();
        let started = Instant::now();
        let outcome = self
            .script_runtime
            .as_mut()
            .map(|runtime| runtime.deliver_file_picker_selection(request_id, selection))
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
