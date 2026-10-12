//! Bounded browser-to-renderer input delivery and deferred History retry.

use super::*;
use crate::windows_app::renderer_input_queue::{KeyboardAdmission, PendingRendererInputs};
use better_web_browser::limits::MAX_QUEUED_BROWSER_COMMANDS;

impl BrowserState {
    pub(in crate::windows_app) fn submit_renderer_input(&mut self, input: DocumentInput) -> bool {
        if let Err(error) = input.validate() {
            unsafe {
                self.contain_page_engine_failure(
                    self.id,
                    format!("invalid document input: {error}"),
                );
            }
            return false;
        }
        let keyboard = KeyboardAdmission::from_input(&input);
        if !self
            .pending_renderer_inputs
            .can_accept_keyboard(keyboard.as_ref())
        {
            unsafe {
                self.set_status(
                    "Too many held keys; this press was not accepted. Release a key and try again.",
                );
            }
            return false;
        }
        if let DocumentInput::Wheel(wheel) = &input
            && self.navigation.owns_document(wheel.document)
        {
            unsafe { self.observe_native_wheel(wheel.sequence, wheel.delta_y) };
        }
        let move_key = PendingRendererInputs::move_key(&input);
        // The broker discards inputs for documents that are not active yet. Such
        // a move cannot supply a completion signal; wait for the first paint.
        if move_key.is_some_and(|(document, _)| {
            self.renderer_revision == 0 || !self.navigation.owns_document(document)
        }) {
            return false;
        }
        let result = if self.pending_renderer_inputs.is_empty()
            && self.pending_renderer_inputs.can_send(&input)
        {
            self.renderer_session
                .as_ref()
                .ok_or_else(|| "renderer session is unavailable".to_string())
                .and_then(|session| session.try_send_input_retained(input))
        } else {
            Ok(Some(input))
        };
        match result {
            Ok(None) => {
                self.pending_renderer_inputs.note_direct_keyboard(keyboard);
                self.pending_renderer_inputs.note_sent(move_key);
                self.note_renderer_input_activity();
                true
            }
            Ok(Some(input)) => match self.pending_renderer_inputs.enqueue(input) {
                QueueResult::Queued | QueueResult::Coalesced => {
                    self.note_renderer_input_activity();
                    true
                }
                QueueResult::Full => {
                    self.note_renderer_input_activity();
                    unsafe {
                        self.set_status(
                            "Renderer is busy; this input was not accepted. Try again.",
                        );
                    }
                    false
                }
            },
            Err(error) => {
                unsafe {
                    self.contain_page_engine_failure(
                        self.id,
                        format!("could not deliver document input: {error}"),
                    );
                }
                false
            }
        }
    }

    fn note_renderer_input_activity(&mut self) {
        self.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
        unsafe { self.ensure_renderer_monitoring() };
    }

    pub(in crate::windows_app) unsafe fn flush_renderer_inputs_for(&mut self, id: TabId) {
        let mut made_room = false;
        for _ in 0..MAX_QUEUED_BROWSER_COMMANDS {
            let delivery = {
                let Some(tab) = self.tabs.get_mut(id) else {
                    return;
                };
                let Some(input) = tab.pending_renderer_inputs.pop_front() else {
                    break;
                };
                if !tab.navigation.owns_document(input.document()) {
                    tab.pending_renderer_inputs
                        .note_delivered(input.document(), input.sequence());
                    made_room = true;
                    continue;
                }
                let move_key = PendingRendererInputs::move_key(&input);
                let input_key = (input.document(), input.sequence());
                let result = tab
                    .renderer_session
                    .as_ref()
                    .ok_or_else(|| "renderer session is unavailable".to_string())
                    .and_then(|session| session.try_send_input_retained(input));
                (result, move_key, input_key)
            };
            match delivery {
                (Ok(None), move_key, input_key) => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.pending_renderer_inputs
                            .note_delivered(input_key.0, input_key.1);
                        tab.pending_renderer_inputs.note_sent(move_key);
                    }
                    made_room = true;
                }
                (Ok(Some(input)), _, _) => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.pending_renderer_inputs.restore_front(input);
                        tab.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
                    }
                    break;
                }
                (Err(error), _, _) => {
                    self.contain_page_engine_failure(
                        id,
                        format!("could not deliver document input: {error}"),
                    );
                    return;
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(id)
            && !tab.pending_renderer_inputs.is_empty()
        {
            tab.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
        }
        // A queued History delta can have failed admission while this input queue was full.
        // Silent inputs need not produce renderer reports, so retry when draining makes room.
        if self
            .tabs
            .get_mut(id)
            .is_some_and(|tab| should_retry_deferred_history(made_room, &tab.history_traversals))
        {
            self.process_for_tab(id, |state| {
                if let Some(document) = state.navigation.active_document() {
                    state.acknowledge_history_traversal(document, None);
                }
            });
        }
    }
}

fn should_retry_deferred_history(
    made_room: bool,
    queue: &crate::windows_app::tab_state::HistoryTraversalQueue,
) -> bool {
    made_room && queue.in_flight.is_none() && !queue.deferred.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::tab_state::HistoryTraversalQueue;

    #[test]
    fn deferred_history_waits_for_input_room_and_no_inflight_step() {
        let mut queue = HistoryTraversalQueue::default();
        queue.deferred.push_back(-1);
        assert!(!should_retry_deferred_history(false, &queue));
        assert!(should_retry_deferred_history(true, &queue));
        queue.in_flight = Some((
            better_web_browser::renderer_protocol::DocumentId::new(1).unwrap(),
            7,
        ));
        assert!(!should_retry_deferred_history(true, &queue));
        queue.clear();
        assert!(!should_retry_deferred_history(true, &queue));
    }
}
