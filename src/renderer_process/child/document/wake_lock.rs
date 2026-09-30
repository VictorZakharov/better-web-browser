use super::*;
use crate::engine::UserInputEvent;
use crate::renderer_protocol::{WakeLockDisposition, WakeLockUpdate};

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn apply_wake_lock_update(
        &mut self,
        update: WakeLockUpdate,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        if update.document != self.id {
            return Ok(None);
        }
        let disposition = match update.disposition {
            WakeLockDisposition::Granted => "granted",
            WakeLockDisposition::Denied => "denied",
            WakeLockDisposition::Released => "released",
        };
        let mut outcome = self
            .dispatch_user_input(UserInputEvent::WakeLock {
                request_id: update.request_id,
                disposition,
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        self.presentation_after_user_input(outcome, false, false, connection)
    }
}
