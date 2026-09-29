//! Selection-only EDIT input and document-scoped renderer selection mirrors.

use super::*;
use better_web_browser::renderer_protocol::{
    MAX_PENDING_TEXT_SELECTIONS, TextSelectionDirection, TextSelectionInput, TextSelectionUpdate,
};

fn sequence_is_current(observed: u64, issued: u64, native: u64, previous: u64) -> bool {
    observed <= issued && observed >= native && observed >= previous
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn route_page_control_selection(
        &mut self,
        id: usize,
        window: Hwnd,
    ) {
        if self.suppress_page_control_focus
            || self.processing_background_tab
            || self.surface != Surface::Page
            || self.renderer_revision == 0
        {
            return;
        }
        let Some(index) = id.checked_sub(ID_PAGE_CONTROL_BASE) else {
            return;
        };
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        if control.window != window || !page_controls::selection::is_text_edit(control.spec.kind) {
            return;
        }
        let native = page_controls::selection::read_edit_selection(window);
        if native == control.last_native_selection {
            return;
        }
        let (value, start, end) =
            page_controls::selection::edit_text_and_selection(window, control.spec.kind);
        if value != control.last_text || (start, end) == control.last_selection {
            return;
        }
        let Some(target) = wire_node(control.spec.node_id) else {
            return;
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        if self.submit_renderer_input(DocumentInput::Selection(TextSelectionInput {
            document,
            sequence,
            target,
            selection_start: start,
            selection_end: end,
            direction: TextSelectionDirection::None,
        })) && let Some(control) = self.page_controls.get_mut(index)
        {
            control.last_selection = (start, end);
            control.last_native_selection = native;
            control.last_direction = TextSelectionDirection::None;
            control.last_native_input_sequence = sequence;
        }
    }

    pub(in crate::windows_app) unsafe fn apply_text_selection_update(
        &mut self,
        update: TextSelectionUpdate,
    ) {
        if update.validate().is_err()
            || self.navigation.active_document() != Some(update.document)
            || update.observed_input_sequence > self.renderer_input_sequence
            || self
                .pending_text_selections
                .get(&update.target)
                .is_some_and(|previous| {
                    update.observed_input_sequence < previous.observed_input_sequence
                })
        {
            return;
        }
        let matching = (self.renderer_revision != 0)
            .then(|| {
                self.page_controls
                    .iter()
                    .position(|control| wire_node(control.spec.node_id) == Some(update.target))
            })
            .flatten();
        if let Some(index) = matching {
            let control = &self.page_controls[index];
            if !page_controls::selection::is_text_edit(control.spec.kind)
                || !sequence_is_current(
                    update.observed_input_sequence,
                    self.renderer_input_sequence,
                    control.last_native_input_sequence,
                    control.last_renderer_selection_sequence,
                )
            {
                self.pending_text_selections.remove(&update.target);
                return;
            }
            let window = control.window;
            let kind = control.spec.kind;
            let previous_suppression = self.suppress_page_control_focus;
            self.suppress_page_control_focus = true;
            page_controls::selection::sync_html_value(window, kind, &update.value);
            let applied = page_controls::selection::apply_html_selection(
                window,
                kind,
                update.selection_start,
                update.selection_end,
                update.direction == TextSelectionDirection::Backward,
            );
            self.suppress_page_control_focus = previous_suppression;
            let native = page_controls::selection::read_edit_selection(window);
            self.page_controls[index]
                .last_text
                .clone_from(&update.value);
            self.page_controls[index]
                .spec
                .value
                .clone_from(&update.value);
            self.page_controls[index].last_selection = applied;
            self.page_controls[index].last_native_selection = native;
            self.page_controls[index].last_direction = update.direction;
            self.page_controls[index].last_renderer_selection_sequence =
                update.observed_input_sequence;
            self.pending_text_selections.remove(&update.target);
            return;
        }
        // A parser script can set selection before the first native ControlSpec.
        // Retain only a bounded latest-per-node mirror for this document.
        let retained_bytes = self
            .pending_text_selections
            .values()
            .filter(|previous| previous.target != update.target)
            .map(|previous| previous.value.len())
            .sum::<usize>();
        if retained_bytes.saturating_add(update.value.len())
            <= better_web_browser::limits::MAX_RENDERER_TEXT_INPUT_BYTES
            && (self.pending_text_selections.contains_key(&update.target)
                || self.pending_text_selections.len() < MAX_PENDING_TEXT_SELECTIONS)
        {
            self.pending_text_selections.insert(update.target, update);
        }
    }

    pub(in crate::windows_app) unsafe fn apply_pending_text_selections(&mut self) {
        let pending = std::mem::take(&mut self.pending_text_selections);
        for update in pending.into_values() {
            self.apply_text_selection_update(update);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sequence_is_current;

    #[test]
    fn selection_mirrors_cannot_claim_future_input_or_regress_accepted_input() {
        assert!(sequence_is_current(0, 0, 0, 0));
        assert!(sequence_is_current(8, 9, 7, 8));
        // Equal sequences allow multiple script changes within one renderer task.
        assert!(sequence_is_current(8, 9, 8, 8));
        assert!(!sequence_is_current(10, 9, 0, 0));
        assert!(!sequence_is_current(6, 9, 7, 0));
        assert!(!sequence_is_current(7, 9, 0, 8));
    }
}
