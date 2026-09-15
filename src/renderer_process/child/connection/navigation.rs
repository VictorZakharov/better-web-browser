//! Main-response input validation and runtime startup before network EOF.
use super::*;
use crate::renderer_process::child::document::{DocumentRuntime, LoadResult, RendererTextSystem};
use crate::renderer_protocol::{NavigationCause, NavigationDisposition, TransferChunk};
use std::panic::{AssertUnwindSafe, catch_unwind};

impl ChildConnection {
    pub(super) fn cancel_navigation(&mut self, document: DocumentId) -> Result<(), String> {
        if self
            .streaming_document
            .is_some_and(|(id, _)| id == document)
        {
            self.streaming_document = None;
        }
        if self
            .incoming_document
            .as_ref()
            .is_some_and(|input| input.start.document == document)
        {
            self.incoming_document = None;
        }
        self.retire_video();
        self.cancel_document_fetches(document);
        if self
            .incoming_storage_update
            .as_ref()
            .is_some_and(|update| update.document() == document)
        {
            self.incoming_storage_update = None;
        }
        if self
            .document
            .as_ref()
            .is_some_and(|runtime| runtime.id() == document)
        {
            self.prepared_text = self.document.take().map(DocumentRuntime::into_text);
        }
        Ok(())
    }

    pub(super) fn abort_navigation(
        &mut self,
        document: DocumentId,
        message: String,
    ) -> Result<(), String> {
        if self
            .streaming_document
            .is_some_and(|(id, _)| id == document)
        {
            self.streaming_document = None;
            self.incoming_document = None;
            self.send_document_failure(document, format!("navigation response failed: {message}"))?;
        }
        Ok(())
    }

    pub(super) fn begin_streaming_document(&mut self, start: DocumentStart) -> Result<(), String> {
        if start.body_length != 0 {
            return Err("streaming document declared a fixed length".into());
        }
        let document = start.document;
        let decoder = crate::winhttp::DocumentDecoder::new(&start.content_type);
        self.begin_document(start)?;
        self.incoming_document
            .as_mut()
            .expect("new document")
            .decoder = Some(decoder);
        self.streaming_document = Some((document, 0));
        Ok(())
    }

    pub(super) fn navigation_chunk(&mut self, chunk: TransferChunk) -> Result<(), String> {
        let (document, offset) = self
            .streaming_document
            .as_mut()
            .ok_or("unsolicited navigation chunk")?;
        if document.get() != chunk.transfer_id || *offset != chunk.offset as usize {
            return Err("navigation chunk identity or offset mismatch".into());
        }
        *offset = offset
            .checked_add(chunk.bytes.len())
            .ok_or("navigation offset overflow")?;
        if *offset > crate::limits::MAX_RESPONSE_BODY_BYTES {
            return Err("navigation input exceeded its byte limit".into());
        }
        self.deliver_navigation_bytes(&chunk.bytes, false)
    }

    pub(super) fn navigation_end(&mut self, document: DocumentId) -> Result<(), String> {
        if !self
            .streaming_document
            .is_some_and(|(id, _)| id == document)
        {
            return Err("navigation completion identity mismatch".into());
        }
        self.deliver_navigation_bytes(&[], true)?;
        self.streaming_document = None;
        Ok(())
    }

    fn deliver_navigation_bytes(&mut self, bytes: &[u8], eof: bool) -> Result<(), String> {
        let document = self.streaming_document.expect("active transfer").0;
        if self.failed_document == Some(document) {
            return Ok(());
        }
        if let Some(input) = self.incoming_document.as_mut() {
            let Some(decoded) = input
                .decoder
                .as_mut()
                .ok_or("missing navigation decoder")?
                .push(bytes, eof)?
            else {
                return Ok(());
            };
            let input = self.incoming_document.take().expect("pending document");
            let state = input.state.finish()?;
            let text = self
                .prepared_text
                .take()
                .unwrap_or_else(|| RendererTextSystem::new(input.start.viewport.dpi));
            let result = catch_unwind(AssertUnwindSafe(|| {
                DocumentRuntime::load_stream(
                    input.start,
                    state,
                    input.decoder.expect("stream decoder"),
                    decoded,
                    self,
                    text,
                )
            }))
            .unwrap_or_else(|payload| Err(runtime::panic_detail(payload)));
            return self.install_streamed_document(document, result);
        }
        let Some(runtime) = self.document.as_mut() else {
            return Ok(());
        };
        if runtime.id() != document {
            return Err("navigation runtime identity mismatch".into());
        }
        if let Err(error) = runtime.append_navigation(bytes, eof) {
            return self.send_document_failure(document, error);
        }
        self.advance_document(document, 0, 0)
    }

    pub(super) fn install_streamed_document(
        &mut self,
        document: DocumentId,
        result: Result<LoadResult, String>,
    ) -> Result<(), String> {
        match result {
            Ok(LoadResult::Ready(runtime, update)) => {
                self.send_document_update(update)?;
                self.document = Some(*runtime);
            }
            Ok(LoadResult::Navigate(url, text)) => {
                self.prepared_text = Some(*text);
                self.writer
                    .send_renderer(&RendererMessage::NavigationRequested {
                        document,
                        url,
                        disposition: NavigationDisposition::CurrentTab,
                        cause: NavigationCause::Redirect,
                    })
                    .map_err(|error| error.to_string())?;
            }
            Err(error) => self.send_document_failure(document, error)?,
        }
        Ok(())
    }
}
