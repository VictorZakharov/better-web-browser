//! Document-scoped speech command and update delivery.

use super::*;
use crate::renderer_protocol::{SpeechRequest, SpeechUpdate};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_speech_request(
        &mut self,
        request: SpeechRequest,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        self.writer
            .send_renderer(&RendererMessage::SpeechRequest(request))
            .map_err(|error| error.to_string())
    }

    pub(super) fn deliver_speech_update(&mut self, update: SpeechUpdate) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != update.document {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = update.document;
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_speech_update(update, self)
        }));
        match result {
            Ok(Ok(Some(update))) => self.send_document_update(update)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => return self.send_document_failure(document, error),
            Err(payload) => {
                return self.send_document_failure(document, super::runtime::panic_detail(payload));
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }
}
