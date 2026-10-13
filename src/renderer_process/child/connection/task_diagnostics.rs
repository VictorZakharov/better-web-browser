//! Task-stage diagnostics for the contained renderer endpoint.
use super::*;

impl ChildConnection {
    pub(in crate::renderer_process::child) fn report_renderer_task_stage(
        &mut self,
        stage: impl Into<String>,
    ) -> Result<(), String> {
        let diagnostic = RendererDiagnostic::new(RENDERER_DIAGNOSTIC_TASK_STAGE, stage)
            .map_err(|error| error.to_string())?;
        self.writer
            .send_renderer(&RendererMessage::Diagnostic(diagnostic))
            .map_err(|error| error.to_string())
    }

    pub(super) fn renderer_task_label(&self, message: &BrowserMessage) -> Option<String> {
        let label = match message {
            BrowserMessage::EndDocument(document) => format!(
                "loading and presenting {}",
                self.document_context(*document)
            ),
            BrowserMessage::StorageSnapshotEnd(update) => format!(
                "installing storage for {}",
                self.document_context(update.document)
            ),
            BrowserMessage::AdvanceTime { document, .. } => format!(
                "running timers, promise jobs, and rendering mutations for {}",
                self.document_context(*document)
            ),
            BrowserMessage::ViewportChanged { document, .. } => format!(
                "laying out a viewport change for {}",
                self.document_context(*document)
            ),
            BrowserMessage::Input(input) => format!(
                "dispatching input and rendering mutations for {}",
                self.document_context(input.document())
            ),
            BrowserMessage::FetchResponseChunk(_) => "delivering a streamed response chunk".into(),
            BrowserMessage::FetchResponseEnd(_) => "completing a streamed response".into(),
            BrowserMessage::FetchResponseAbort(_) => "aborting a streamed response".into(),
            BrowserMessage::Test(command) => format!("running renderer test command {command:?}"),
            _ => return None,
        };
        Some(label)
    }

    fn document_context(&self, document: DocumentId) -> String {
        let url = self
            .incoming_document
            .as_ref()
            .filter(|incoming| incoming.start.document == document)
            .map(|incoming| incoming.start.url.as_str())
            .or_else(|| {
                self.document
                    .as_ref()
                    .filter(|runtime| runtime.id() == document)
                    .map(DocumentRuntime::source_url)
            });
        match url {
            Some(url) => format!("document {} ({url})", document.get()),
            None => format!("document {}", document.get()),
        }
    }
}
