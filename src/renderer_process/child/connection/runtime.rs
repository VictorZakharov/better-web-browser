//! Panic containment and lifecycle for work performed by the active document.

use super::{ChildConnection, IncomingDocument, ScriptFetchDelivery};
use crate::limits::{MAX_RESPONSE_BODY_BYTES, MAX_SCRIPT_LOOP_ITERATIONS};
use crate::renderer_process::child::document::{
    AdvanceResult, DocumentRuntime, LoadResult, RendererTextSystem,
};
use crate::renderer_protocol::{
    DocumentId, DocumentInput, DocumentStart, FullscreenResponse, NavigationCause,
    NavigationDisposition, PresentationAcknowledgement, RendererMessage, TransferAssembler,
    TransferChunk,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Duration;

impl ChildConnection {
    pub(super) fn complete_document_resource_preloads(&mut self) -> Result<(), String> {
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        let document = runtime.id();
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.finish_completed_resource_preloads(self)
        }));
        match result {
            Ok(Ok(Some(presentation))) => self.send_presentation(&presentation)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn deliver_script_fetch(
        &mut self,
        delivery: ScriptFetchDelivery,
    ) -> Result<(), String> {
        let document = delivery.document();
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.deliver_script_fetch(delivery, self)
        }));
        match result {
            Ok(Ok(Some(AdvanceResult::Presentation(presentation)))) => {
                self.send_presentation(&presentation)?;
            }
            Ok(Ok(Some(AdvanceResult::Runtime(update)))) => self
                .writer
                .send_renderer(&RendererMessage::RuntimeUpdate(update))
                .map_err(|error| error.to_string())?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn begin_document(&mut self, start: DocumentStart) -> Result<(), String> {
        start.validate().map_err(|error| error.to_string())?;
        if self.incoming_document.is_some() || self.document.is_some() {
            return Err("renderer already owns a document".into());
        }
        self.failed_document = None;
        self.incoming_document = Some(IncomingDocument {
            body: TransferAssembler::new(
                start.document.get(),
                start.body_length as usize,
                MAX_RESPONSE_BODY_BYTES,
            )
            .map_err(|error| error.to_string())?,
            state: super::IncomingDocumentState::new(start.document),
            start,
        });
        Ok(())
    }

    pub(super) fn document_chunk(&mut self, chunk: TransferChunk) -> Result<(), String> {
        self.incoming_document
            .as_mut()
            .ok_or_else(|| "unsolicited document chunk".to_string())?
            .body
            .push(chunk)
            .map_err(|error| error.to_string())
    }

    pub(super) fn finish_document(&mut self, document: DocumentId) -> Result<(), String> {
        let incoming = self
            .incoming_document
            .take()
            .ok_or_else(|| "unsolicited document completion".to_string())?;
        if incoming.start.document != document {
            return Err("document completion identity mismatch".into());
        }
        let body = incoming
            .body
            .finish(document.get())
            .map_err(|error| error.to_string())?;
        let state = incoming.state.finish()?;
        let start = incoming.start;
        let text = self
            .prepared_text
            .take()
            .unwrap_or_else(|| RendererTextSystem::new(start.viewport.dpi));
        let result = catch_unwind(AssertUnwindSafe(|| {
            DocumentRuntime::load(start, state, body, self, text)
        }));
        match result {
            Ok(Ok(LoadResult::Ready(runtime, presentation))) => {
                self.send_presentation(&presentation)?;
                self.document = Some(*runtime);
            }
            Ok(Ok(LoadResult::Navigate(url, text))) => {
                self.prepared_text = Some(*text);
                self.writer
                    .send_renderer(&RendererMessage::NavigationRequested {
                        document,
                        url,
                        disposition: NavigationDisposition::CurrentTab,
                        cause: NavigationCause::Redirect,
                    })
                    .map_err(|error| error.to_string())?
            }
            Ok(Err(error)) => self.send_document_failure(document, error)?,
            Err(payload) => self.send_document_failure(document, panic_detail(payload))?,
        }
        Ok(())
    }

    pub(super) fn advance_document(
        &mut self,
        document: DocumentId,
        elapsed_micros: u64,
        max_callbacks: u32,
    ) -> Result<(), String> {
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let elapsed = Duration::from_micros(elapsed_micros.min(60_000_000));
        let max_callbacks = max_callbacks.min(MAX_SCRIPT_LOOP_ITERATIONS as u32);
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.advance(elapsed, max_callbacks, self)
        }));
        match result {
            Ok(Ok(AdvanceResult::Presentation(presentation))) => {
                self.send_presentation(&presentation)?
            }
            Ok(Ok(AdvanceResult::Runtime(update))) => self
                .writer
                .send_renderer(&RendererMessage::RuntimeUpdate(update))
                .map_err(|error| error.to_string())?,
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        };
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn resize_document(
        &mut self,
        document: DocumentId,
        viewport: crate::renderer_protocol::PresentedViewport,
    ) -> Result<(), String> {
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() == document {
            match catch_unwind(AssertUnwindSafe(|| runtime.resize(viewport, self))) {
                Ok(Ok(presentation)) => self.send_presentation(&presentation)?,
                Ok(Err(error)) => {
                    self.send_document_failure(document, error)?;
                    return Ok(());
                }
                Err(payload) => {
                    self.send_document_failure(document, panic_detail(payload))?;
                    return Ok(());
                }
            }
        }
        self.document = Some(runtime);
        Ok(())
    }

    pub(super) fn interact_document(&mut self, input: DocumentInput) -> Result<(), String> {
        let document = input.document();
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let result = catch_unwind(AssertUnwindSafe(|| runtime.interact(input, self)));
        match result {
            Ok(Ok(result)) => {
                if let Some(cursor) = result.cursor {
                    self.writer
                        .send_renderer(&RendererMessage::PointerCursor(cursor))
                        .map_err(|error| error.to_string())?;
                }
                if let Some(presentation) = result.presentation {
                    self.send_presentation(&presentation)?;
                }
                if let Some((url, disposition)) = result.navigation {
                    self.writer
                        .send_renderer(&RendererMessage::NavigationRequested {
                            document,
                            url,
                            disposition,
                            cause: NavigationCause::UserActivation,
                        })
                        .map_err(|error| error.to_string())?;
                }
            }
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn fullscreen_response(
        &mut self,
        response: FullscreenResponse,
    ) -> Result<(), String> {
        let response = response.validate().map_err(|error| error.to_string())?;
        let document = response.document;
        let Some(mut runtime) = self.document.take() else {
            return Ok(());
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            return Ok(());
        }
        let result = catch_unwind(AssertUnwindSafe(|| {
            runtime.apply_fullscreen_response(response, self)
        }));
        match result {
            Ok(Ok(Some(presentation))) => self.send_presentation(&presentation)?,
            Ok(Ok(None)) => {}
            Ok(Err(error)) => {
                self.send_document_failure(document, error)?;
                return Ok(());
            }
            Err(payload) => {
                self.send_document_failure(document, panic_detail(payload))?;
                return Ok(());
            }
        }
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    pub(super) fn acknowledge_presentation(
        &mut self,
        acknowledgement: PresentationAcknowledgement,
    ) -> Result<(), String> {
        let Some(runtime) = self.document.as_mut() else {
            return Ok(());
        };
        if runtime.id() == acknowledgement.document {
            runtime.acknowledge_presentation(acknowledgement)?;
        }
        Ok(())
    }
}

fn panic_detail(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        format!("document task panicked: {message}")
    } else if let Some(message) = payload.downcast_ref::<String>() {
        format!("document task panicked: {message}")
    } else {
        "document task panicked".into()
    }
}
