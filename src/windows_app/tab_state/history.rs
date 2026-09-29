//! Session-history entries and their logical document-state ownership.

use super::BrowserTab;
use better_web_browser::limits::MAX_SESSION_HISTORY_ENTRIES;
use better_web_browser::renderer_protocol::DocumentId;
use better_web_browser::renderer_protocol::ScrollRestorationMode;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_HISTORY_DOCUMENT_GROUP: AtomicU64 = AtomicU64::new(1);

/// One session-history entry. `document` identifies only a currently live renderer document;
/// `group` preserves the shared document state after that renderer has been retired.
/// See HTML Standard §7.4.1.2, where pushState entries share their document state.
#[derive(Clone, Debug, PartialEq)]
pub(in crate::windows_app) struct HistoryEntry {
    pub(in crate::windows_app) url: String,
    pub(in crate::windows_app) document: Option<DocumentId>,
    pub(in crate::windows_app) group: HistoryDocumentGroup,
    pub(in crate::windows_app) state: Option<String>,
    pub(in crate::windows_app) scroll_restoration: ScrollRestorationMode,
    /// Last viewport Y in CSS pixels, saved separately for each session-history entry.
    pub(in crate::windows_app) scroll_y: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(in crate::windows_app) struct HistoryDocumentGroup(u64);

impl HistoryDocumentGroup {
    fn new() -> Self {
        Self(NEXT_HISTORY_DOCUMENT_GROUP.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Default)]
pub(in crate::windows_app) struct HistoryTraversalQueue {
    pub(in crate::windows_app) in_flight: Option<(DocumentId, u64)>,
    pub(in crate::windows_app) deferred: VecDeque<i32>,
}

impl HistoryTraversalQueue {
    pub(in crate::windows_app) fn clear(&mut self) {
        self.in_flight = None;
        self.deferred.clear();
    }
}

impl HistoryEntry {
    pub(in crate::windows_app) fn new(url: String) -> Self {
        Self {
            url,
            document: None,
            group: HistoryDocumentGroup::new(),
            state: None,
            scroll_restoration: ScrollRestorationMode::Auto,
            scroll_y: None,
        }
    }

    pub(in crate::windows_app) fn same_document(
        url: String,
        current: &Self,
        document: DocumentId,
        state: Option<String>,
    ) -> Self {
        Self {
            url,
            document: Some(document),
            group: current.group,
            state,
            scroll_restoration: current.scroll_restoration,
            scroll_y: current.scroll_y,
        }
    }
}

impl BrowserTab {
    pub(in crate::windows_app) fn current_url(&self) -> Option<&str> {
        self.history
            .get(self.history_index)
            .map(|entry| entry.url.as_str())
            .filter(|url| !url.is_empty())
    }

    pub(in crate::windows_app) fn push_history_entry(&mut self, entry: HistoryEntry) {
        self.history.truncate(self.history_index.saturating_add(1));
        self.history.push(entry);
        if self.history.len() > MAX_SESSION_HISTORY_ENTRIES {
            self.history.remove(0);
        }
        self.history_index = self.history.len() - 1;
    }

    pub(in crate::windows_app) fn replace_current_history_url(&mut self, url: String) {
        if let Some(current) = self.history.get_mut(self.history_index) {
            *current = HistoryEntry::new(url);
        } else {
            self.push_history_entry(HistoryEntry::new(url));
        }
    }

    pub(in crate::windows_app) fn retire_history_documents(&mut self) {
        for entry in &mut self.history {
            entry.document = None;
        }
    }

    pub(in crate::windows_app) fn commit_history_document(
        &mut self,
        final_url: &str,
        redirected: bool,
        document: DocumentId,
    ) {
        let Some(current) = self.history.get_mut(self.history_index) else {
            return;
        };
        if redirected || current.url != final_url {
            current.group = HistoryDocumentGroup::new();
            current.state = None;
        }
        current.url = final_url.to_owned();
        let group = current.group;
        for entry in &mut self.history {
            if entry.group == group {
                entry.document = Some(document);
            }
        }
    }
}
