use super::*;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn accessibility_enabled(&self) -> bool {
        self.accessibility_enabled
    }

    pub(super) fn accessibility_policy(&mut self, enabled: bool) -> Result<(), String> {
        self.accessibility_enabled = enabled;
        let Some(runtime) = self.document.as_mut() else {
            return Ok(());
        };
        if runtime.set_accessibility_enabled(enabled) {
            // Publish the document's first full tree without waiting for its next change.
            let document = runtime.id();
            self.advance_document_with_clock(document, 0, 1, false)?;
        }
        Ok(())
    }

    pub(super) fn gpu_canvas_policy(
        &mut self,
        document: DocumentId,
        enabled: bool,
    ) -> Result<(), String> {
        if let Some(runtime) = self
            .document
            .as_mut()
            .filter(|runtime| runtime.id() == document)
        {
            runtime.set_gpu_canvas_enabled(enabled);
        } else {
            return Ok(());
        }
        if !enabled {
            // A static unpreserved Canvas may have no future author callback.
            // Run its requested repair checkpoint now without acknowledging a
            // separately queued browser clock advance.
            self.advance_document_with_clock(document, 0, 1, false)?;
        }
        Ok(())
    }
}
