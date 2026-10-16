//! Input completion, including mousemove transport acknowledgments on silent paths.

use super::ChildConnection;
use crate::renderer_protocol::{
    DocumentInput, NavigationCause, PointerCursor, PointerCursorResult, PointerPhase,
    RendererMessage,
};
use std::panic::{AssertUnwindSafe, catch_unwind};

#[cfg(test)]
mod tests;

impl ChildConnection {
    pub(super) fn interact_document(&mut self, input: DocumentInput) -> Result<(), String> {
        let document = input.document();
        let move_completion = default_move_completion(&input);
        let scroll_completion = match &input {
            DocumentInput::Scroll(scroll) => Some((scroll.document, scroll.sequence)),
            _ => None,
        };
        let Some(mut runtime) = self.document.take() else {
            self.send_move_completion(move_completion)?;
            return self.send_scroll_completion(scroll_completion);
        };
        if runtime.id() != document {
            self.document = Some(runtime);
            self.send_move_completion(move_completion)?;
            return self.send_scroll_completion(scroll_completion);
        }
        let result = catch_unwind(AssertUnwindSafe(|| runtime.interact(input, self)));
        match result {
            Ok(Ok(result)) => {
                // Cursor is also the exact document/sequence completion signal for
                // browser-side motion backpressure. Scrollbar handling and rejected
                // stale input have no DOM cursor result, but must still release it.
                self.send_move_completion(result.cursor.or(move_completion))?;
                if let Some(presentation) = result.presentation {
                    self.send_document_update(presentation)?;
                } else if let Some(update) = runtime.pending_geometry_observer_update() {
                    // A scroll-only input has no visual revision, but its observer task must
                    // still wake an otherwise idle browser-owned document clock.
                    self.writer
                        .send_renderer(&RendererMessage::RuntimeUpdate(Box::new(update)))
                        .map_err(|error| error.to_string())?;
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
                self.send_document_failure(document, super::runtime::panic_detail(payload))?;
                return Ok(());
            }
        }
        self.send_scroll_completion(scroll_completion)?;
        if !self.stopping {
            self.document = Some(runtime);
        }
        Ok(())
    }

    fn send_move_completion(&self, result: Option<PointerCursorResult>) -> Result<(), String> {
        if let Some(result) = result {
            self.writer
                .send_renderer(&RendererMessage::PointerCursor(result))
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }

    fn send_scroll_completion(
        &self,
        completion: Option<(crate::renderer_protocol::DocumentId, u64)>,
    ) -> Result<(), String> {
        if let Some((document, sequence)) = completion {
            self.writer
                .send_renderer(&RendererMessage::ScrollCompleted { document, sequence })
                .map_err(|error| error.to_string())?;
        }
        Ok(())
    }
}

fn default_move_completion(input: &DocumentInput) -> Option<PointerCursorResult> {
    match input {
        DocumentInput::Pointer(input) if input.phase == PointerPhase::Move => {
            Some(PointerCursorResult {
                document: input.document,
                sequence: input.sequence,
                cursor: PointerCursor::Default,
            })
        }
        _ => None,
    }
}
