//! Tab session history and same-document traversal admission.

use super::*;
use better_web_browser::limits::{MAX_HISTORY_STATE_BYTES, MAX_PENDING_RENDERER_INPUTS};
use better_web_browser::navigation::can_rewrite_history_url;
use better_web_browser::renderer_protocol::{DocumentInput, HistoryAction, HistoryTraversalInput};

use crate::windows_app::tab_state::HistoryEntry;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TraversalResult {
    NoChange,
    SameDocument,
    NewDocument,
    AdmissionFailed,
}

impl BrowserState {
    pub(in crate::windows_app) unsafe fn apply_same_document_history_updates(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        actions: &[HistoryAction],
    ) {
        if !self.navigation.owns_document(document) {
            return;
        }
        for action in actions {
            if !self.navigation.owns_document(document) {
                break;
            }
            if let HistoryAction::Update {
                url,
                replace,
                state,
            } = action
            {
                self.apply_history_update(document, url, *replace, state.as_deref());
            }
        }
    }

    pub(in crate::windows_app) unsafe fn apply_queued_history_traversals(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        actions: &[HistoryAction],
    ) -> bool {
        // HTML schedules traversal as a later task. All synchronous push/replaceState calls
        // in this renderer report must commit before any queued go/back/forward runs, even if
        // script called go() first and then pushed a new entry before yielding.
        // An earlier report may already be waiting for queue space. New report tasks must
        // join the tail instead of overtaking that deferred traversal.
        let mut admission_blocked = !self.history_traversals.deferred.is_empty();
        for action in actions {
            if !self.navigation.owns_document(document) {
                return true;
            }
            if let HistoryAction::Traverse { delta } = action {
                if admission_blocked {
                    self.defer_history_traversal(*delta);
                    continue;
                }
                match self.traverse_history(*delta) {
                    TraversalResult::NewDocument => return true,
                    TraversalResult::AdmissionFailed => {
                        // The renderer input queue is full. Keep this and later deltas in
                        // report order; the input-drain path retries once it makes room.
                        self.defer_history_traversal(*delta);
                        admission_blocked = true;
                    }
                    TraversalResult::NoChange | TraversalResult::SameDocument => {}
                }
            }
        }
        false
    }

    fn defer_history_traversal(&mut self, delta: i32) {
        if !enqueue_deferred_traversal(&mut self.history_traversals, delta) {
            self.incidents
                .record("history", "deferred traversal queue is full");
        }
    }

    unsafe fn apply_history_update(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        url: &str,
        replace: bool,
        state: Option<&str>,
    ) {
        let valid = self
            .current_url()
            .is_some_and(|current| can_rewrite_history_url(current, url))
            && state.is_none_or(|state| state.len() <= MAX_HISTORY_STATE_BYTES);
        if !valid {
            self.incidents.record(
                "history",
                format!("rejected same-document URL or state: {url}"),
            );
            return;
        }
        let entry = HistoryEntry::same_document(
            url.to_owned(),
            &self.history[self.history_index],
            document,
            state.map(str::to_owned),
        );
        if replace {
            let index = self.history_index;
            if let Some(current) = self.history.get_mut(index) {
                *current = entry;
            } else {
                self.push_history_entry(entry);
            }
        } else {
            self.push_history_entry(entry);
        }
        self.set_history_url(url);
        self.incidents.record(
            "history",
            format!(
                "{} same-document URL: {url}",
                if replace { "replace" } else { "push" }
            ),
        );
    }

    pub(in crate::windows_app) unsafe fn go_back(&mut self) {
        self.traverse_history(-1);
    }

    pub(in crate::windows_app) unsafe fn go_forward(&mut self) {
        self.traverse_history(1);
    }

    pub(in crate::windows_app) unsafe fn reload(&mut self) {
        if let Some(url) = self.current_url().map(str::to_owned) {
            self.begin_navigation(url, HistoryMode::Existing);
        }
    }

    unsafe fn traverse_history(&mut self, delta: i32) -> TraversalResult {
        if let Some((document, _)) = self.history_traversals.in_flight {
            if self.navigation.owns_document(document) {
                // The next step is relative to the entry activated by the pending popstate.
                // Do not resolve its target, or tear down the renderer, until that task ends.
                self.defer_history_traversal(delta);
                return TraversalResult::NoChange;
            }
            self.history_traversals.clear();
        }
        if delta == 0 {
            self.reload();
            return TraversalResult::NewDocument;
        }
        let Some(target_index) =
            history_target_index(self.history_index, self.history.len(), delta)
        else {
            return TraversalResult::NoChange;
        };
        let target = self.history[target_index].clone();
        let active_document = self.navigation.active_document();
        let current_document = self
            .history
            .get(self.history_index)
            .and_then(|entry| entry.document);
        if active_document.is_some()
            && current_document == active_document
            && target.document == active_document
        {
            let Some((document, sequence)) = self.next_renderer_input() else {
                return TraversalResult::AdmissionFailed;
            };
            // Admission is transactional: a full renderer input queue must leave browser history
            // and chrome at the old entry so they cannot diverge from the live document.
            if !self.submit_renderer_input(DocumentInput::History(HistoryTraversalInput {
                document,
                sequence,
                url: target.url.clone(),
                state: target.state,
                history_length: self.history.len() as u32,
                history_index: target_index as u32,
            })) {
                return TraversalResult::AdmissionFailed;
            }
            self.history_traversals.in_flight = Some((document, sequence));
            self.history_index = target_index;
            self.set_history_url(&target.url);
            self.incidents
                .record("history", format!("traverse same document: {}", target.url));
            TraversalResult::SameDocument
        } else {
            self.history_index = target_index;
            self.begin_navigation(target.url, HistoryMode::Existing);
            TraversalResult::NewDocument
        }
    }

    pub(in crate::windows_app) unsafe fn acknowledge_history_traversal(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        sequence: Option<u64>,
    ) -> bool {
        if let Some(sequence) = sequence {
            if self.history_traversals.in_flight != Some((document, sequence)) {
                self.incidents.record(
                    "history",
                    format!("ignored stale traversal acknowledgement: {sequence}"),
                );
                return false;
            }
            self.history_traversals.in_flight = None;
        }
        if self.history_traversals.in_flight.is_some() {
            return false;
        }
        while let Some(delta) = self.history_traversals.deferred.pop_front() {
            match self.traverse_history(delta) {
                TraversalResult::NoChange => continue,
                TraversalResult::SameDocument => return false,
                TraversalResult::NewDocument => return true,
                TraversalResult::AdmissionFailed => {
                    self.history_traversals.deferred.push_front(delta);
                    return false;
                }
            }
        }
        false
    }

    unsafe fn set_history_url(&mut self, url: &str) {
        self.reader_url = url.to_owned();
        self.omnibox_text = url.to_owned();
        if let Some(benchmark) = self.benchmark.as_mut() {
            benchmark.final_url = url.to_owned();
        }
        if !self.processing_background_tab {
            set_window_text(self.controls.address, url);
            self.update_history_buttons();
        }
    }

    pub(in crate::windows_app) unsafe fn update_history_buttons(&self) {
        EnableWindow(self.controls.back, (self.history_index > 0) as i32);
        EnableWindow(
            self.controls.forward,
            (self.history_index + 1 < self.history.len()) as i32,
        );
        EnableWindow(self.controls.reload, (!self.history.is_empty()) as i32);
    }
}

fn history_target_index(index: usize, length: usize, delta: i32) -> Option<usize> {
    let target = index.checked_add_signed(delta as isize)?;
    (target < length).then_some(target)
}

fn enqueue_deferred_traversal(
    queue: &mut crate::windows_app::tab_state::HistoryTraversalQueue,
    delta: i32,
) -> bool {
    if queue.deferred.len() >= MAX_PENDING_RENDERER_INPUTS {
        return false;
    }
    queue.deferred.push_back(delta);
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn out_of_range_traversal_is_a_noop() {
        assert_eq!(history_target_index(0, 3, -1), None);
        assert_eq!(history_target_index(2, 3, 1), None);
        assert_eq!(history_target_index(2, 3, i32::MIN), None);
        assert_eq!(history_target_index(1, 3, -1), Some(0));
        assert_eq!(history_target_index(1, 3, 1), Some(2));
    }

    #[test]
    fn admission_retry_preserves_report_order_and_stays_bounded() {
        let mut queue = crate::windows_app::tab_state::HistoryTraversalQueue::default();
        for delta in [-1, -1, 1] {
            assert!(enqueue_deferred_traversal(&mut queue, delta));
        }
        assert_eq!(queue.deferred.into_iter().collect::<Vec<_>>(), [-1, -1, 1]);

        let mut queue = crate::windows_app::tab_state::HistoryTraversalQueue::default();
        for _ in 0..MAX_PENDING_RENDERER_INPUTS {
            assert!(enqueue_deferred_traversal(&mut queue, -1));
        }
        assert!(!enqueue_deferred_traversal(&mut queue, 1));
        assert_eq!(queue.deferred.len(), MAX_PENDING_RENDERER_INPUTS);
    }
}
