//! Browser-acknowledged Fullscreen API state transitions.

use super::*;
use crate::engine::UserInputEvent;
use crate::renderer_protocol::{FullscreenDisposition, FullscreenResponse};

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn apply_fullscreen_response(
        &mut self,
        response: FullscreenResponse,
        connection: &mut ChildConnection,
    ) -> Result<Option<RendererPresentation>, String> {
        if response.document != self.id {
            return Ok(None);
        }
        let mut outcome = self
            .dispatch_user_input(UserInputEvent::Fullscreen {
                request_id: response.request_id,
                disposition: match response.disposition {
                    FullscreenDisposition::Entered => "entered",
                    FullscreenDisposition::Exited => "exited",
                    FullscreenDisposition::Denied => "denied",
                },
            })?
            .outcome;
        self.admit_user_input_outcome(&mut outcome, connection)?;
        self.presentation_after_user_input(outcome, false, connection)
    }
}
