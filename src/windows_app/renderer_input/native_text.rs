//! Generation-fenced rollback of rejected native text and its selection caches.

use super::*;
use better_web_browser::renderer_protocol::{
    DocumentId, NativeTextRejection, TextSelectionDirection,
};

impl BrowserState {
    pub(in crate::windows_app) unsafe fn apply_native_text_rejection(
        &mut self,
        document: DocumentId,
        rejection: &NativeTextRejection,
    ) {
        if !self.navigation.owns_document(document)
            || rejection.generation != self.native_text_generation
            || rejection.sequence == 0
            || rejection.sequence > self.renderer_input_sequence
        {
            return;
        }
        let Some(next_generation) = self.native_text_generation.checked_add(1) else {
            self.contain_page_engine_failure(self.id, "native text generation overflow".into());
            return;
        };
        self.pending_renderer_inputs
            .discard_native_text_generation(document, rejection.generation);
        self.native_text_generation = next_generation;
        let previous_suppression = self.suppress_page_control_edit;
        self.suppress_page_control_edit = true;
        if let Some(control) = self
            .page_controls
            .iter_mut()
            .find(|control| wire_node(control.spec.node_id) == Some(rejection.target))
        {
            restore_control(control, rejection);
        }
        self.suppress_page_control_edit = previous_suppression;
    }
}

unsafe fn restore_control(
    control: &mut page_controls::PageControlWindow,
    rejection: &NativeTextRejection,
) {
    // SetWindowText produces synchronous EN_CHANGE; the caller suppresses its
    // echo while restoring both the HWND and the browser's authoritative cache.
    page_controls::selection::sync_html_value(control.window, control.spec.kind, &rejection.value);
    let applied = page_controls::selection::apply_html_selection(
        control.window,
        control.spec.kind,
        rejection.selection_start,
        rejection.selection_end,
        false,
    );
    control.last_text.clone_from(&rejection.value);
    control.spec.value.clone_from(&rejection.value);
    control.last_selection = applied;
    control.last_native_selection = page_controls::selection::read_edit_selection(control.window);
    control.last_direction = TextSelectionDirection::None;
    // Queued edits from the rejected generation may have issued later sequence
    // numbers. Never lower their fence and allow stale renderer mirrors through.
    control.last_native_input_sequence = control.last_native_input_sequence.max(rejection.sequence);
    control.last_renderer_selection_sequence = control
        .last_renderer_selection_sequence
        .max(rejection.sequence);
}

#[cfg(test)]
mod tests;
