use super::*;
use crate::renderer_protocol::WakeLockUpdate;
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(super) fn deliver_wake_lock_update(
        &mut self,
        update: WakeLockUpdate,
    ) -> Result<(), String> {
        let update = update.validate().map_err(|error| error.to_string())?;
        let document = update.document;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.apply_wake_lock_update(update, self)
        }));
        match result {
            Ok(Ok(Some(presentation))) => self.send_document_update(presentation)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, super::runtime::panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }
}
