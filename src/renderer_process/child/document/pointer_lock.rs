//! Browser-acknowledged Pointer Lock state transitions.

use super::*;
use crate::engine::UserInputEvent;
use crate::renderer_protocol::{PointerLockDisposition, PointerLockResponse};

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn apply_pointer_lock_response(
        &mut self,
        response: PointerLockResponse,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if response.document != self.id {
            return Ok(None);
        }
        let mut outcome = self
            .dispatch_user_input(UserInputEvent::PointerLock {
                request_id: response.request_id,
                disposition: match response.disposition {
                    PointerLockDisposition::Entered => "entered",
                    PointerLockDisposition::Exited => "exited",
                    PointerLockDisposition::Denied => "denied",
                },
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        self.presentation_after_user_input(outcome, false, connection)
    }
}
