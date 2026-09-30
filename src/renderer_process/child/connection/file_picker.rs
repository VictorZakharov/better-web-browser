//! Document-bound file picker request and reply forwarding.

use super::*;
use crate::renderer_protocol::{FilePickerRequest, FilePickerUpdate};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(in crate::renderer_process::child) fn send_file_picker_request(
        &mut self,
        request: FilePickerRequest,
    ) -> Result<(), String> {
        request.validate().map_err(|error| error.to_string())?;
        self.writer
            .send_renderer(&RendererMessage::FilePickerRequest(request))
            .map_err(|error| error.to_string())
    }

    pub(super) fn deliver_file_picker_update(
        &mut self,
        update: FilePickerUpdate,
    ) -> Result<(), String> {
        update.validate().map_err(|error| error.to_string())?;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != update.document() {
            self.document = Some(runtime);
            return Ok(());
        }
        let document = update.document();
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_file_picker_update(update, self)
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
