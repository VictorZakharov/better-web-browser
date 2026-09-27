//! Bounded browser-to-renderer input delivery and deferred History retry.

use super::*;
use better_web_browser::limits::MAX_QUEUED_BROWSER_COMMANDS;

impl BrowserState {
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
                    made_room = true;
                    continue;
                }
                tab.renderer_session
                    .as_ref()
                    .ok_or_else(|| "renderer session is unavailable".to_string())
                    .and_then(|session| session.try_send_input_retained(input))
            };
            match delivery {
                Ok(None) => made_room = true,
                Ok(Some(input)) => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.pending_renderer_inputs.restore_front(input);
                        tab.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
                    }
                    break;
                }
                Err(error) => {
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
