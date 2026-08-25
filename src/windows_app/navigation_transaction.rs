//! Per-tab authority for one full-document navigation and its renderer attempt.

use super::document_activation::LoadedPage;
use better_web_browser::renderer_protocol::DocumentId;
use std::time::{Duration, Instant};

pub(super) const FIRST_PRESENTATION_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_FIRST_PRESENTATION_RETRIES: u8 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum NavigationPhase {
    Fetching,
    WaitingForRenderer,
    WaitingForFirstPresentation,
    Presented,
    Failed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum PresentationDeadline {
    Retry,
    Failed,
}

pub(super) struct NavigationTransaction {
    generation: u64,
    phase: NavigationPhase,
    page: Option<LoadedPage>,
    document: Option<DocumentId>,
    first_presentation_deadline: Option<Instant>,
    retry_count: u8,
}

impl NavigationTransaction {
    pub(super) fn new(page: LoadedPage) -> Self {
        Self {
            generation: 1,
            phase: NavigationPhase::WaitingForRenderer,
            page: Some(page),
            document: None,
            first_presentation_deadline: None,
            retry_count: 0,
        }
    }

    pub(super) fn begin(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.phase = NavigationPhase::Fetching;
        self.page = None;
        self.document = None;
        self.first_presentation_deadline = None;
        self.retry_count = 0;
        self.generation
    }

    pub(super) fn accept_page(&mut self, generation: u64, page: LoadedPage) -> bool {
        if generation != self.generation || self.phase != NavigationPhase::Fetching {
            return false;
        }
        self.page = Some(page);
        self.phase = NavigationPhase::WaitingForRenderer;
        true
    }

    pub(super) fn page_for_submission(&self) -> Option<LoadedPage> {
        (self.phase == NavigationPhase::WaitingForRenderer)
            .then(|| self.page.clone())
            .flatten()
    }

    pub(super) fn document_id(&self) -> Result<DocumentId, String> {
        DocumentId::new(self.generation).map_err(|error| error.to_string())
    }

    pub(super) fn document_submitted(&mut self, document: DocumentId, now: Instant) -> bool {
        if self.phase != NavigationPhase::WaitingForRenderer || document.get() != self.generation {
            return false;
        }
        self.document = Some(document);
        self.phase = NavigationPhase::WaitingForFirstPresentation;
        self.first_presentation_deadline = Some(now + FIRST_PRESENTATION_TIMEOUT);
        true
    }

    pub(super) fn owns_document(&self, document: DocumentId) -> bool {
        self.document == Some(document)
    }

    pub(super) fn active_document(&self) -> Option<DocumentId> {
        self.document
    }

    pub(super) fn mark_presented(&mut self, document: DocumentId) -> bool {
        if !self.owns_document(document) {
            return false;
        }
        let first = self.phase == NavigationPhase::WaitingForFirstPresentation;
        if first {
            self.phase = NavigationPhase::Presented;
            self.page = None;
            self.first_presentation_deadline = None;
        }
        first
    }

    pub(super) fn deadline(&mut self, now: Instant) -> Option<PresentationDeadline> {
        if self.phase != NavigationPhase::WaitingForFirstPresentation
            || self
                .first_presentation_deadline
                .is_none_or(|deadline| now < deadline)
        {
            return None;
        }
        Some(self.fail_renderer_attempt())
    }

    pub(super) fn renderer_exited(&mut self) -> Option<PresentationDeadline> {
        (self.phase == NavigationPhase::WaitingForFirstPresentation)
            .then(|| self.fail_renderer_attempt())
    }

    fn fail_renderer_attempt(&mut self) -> PresentationDeadline {
        self.document = None;
        self.first_presentation_deadline = None;
        if self.retry_count < MAX_FIRST_PRESENTATION_RETRIES {
            self.retry_count += 1;
            self.phase = NavigationPhase::WaitingForRenderer;
            PresentationDeadline::Retry
        } else {
            self.phase = NavigationPhase::Failed;
            self.page = None;
            PresentationDeadline::Failed
        }
    }

    pub(super) fn fail(&mut self) {
        self.phase = NavigationPhase::Failed;
        self.page = None;
        self.document = None;
        self.first_presentation_deadline = None;
    }

    pub(super) fn invalidate(&mut self) {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.fail();
        self.retry_count = 0;
    }

    pub(super) fn generation(&self) -> u64 {
        self.generation
    }

    pub(super) fn is_loading(&self) -> bool {
        matches!(
            self.phase,
            NavigationPhase::Fetching
                | NavigationPhase::WaitingForRenderer
                | NavigationPhase::WaitingForFirstPresentation
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page() -> LoadedPage {
        LoadedPage {
            body: b"page".to_vec(),
            final_url: "https://example.test/".into(),
            status: 200,
            content_type: "text/html".into(),
            bytes: 4,
            network_time: Duration::ZERO,
        }
    }

    #[test]
    fn stale_fetch_completion_cannot_enter_a_new_transaction() {
        let mut transaction = NavigationTransaction::new(page());
        let stale = transaction.begin();
        let current = transaction.begin();
        assert!(!transaction.accept_page(stale, page()));
        assert!(transaction.accept_page(current, page()));
    }

    #[test]
    fn first_presentation_timeout_retries_once_then_fails_closed() {
        let mut transaction = NavigationTransaction::new(page());
        let now = Instant::now();
        let first = transaction.document_id().unwrap();
        assert!(transaction.document_submitted(first, now));
        assert_eq!(
            transaction.deadline(now + FIRST_PRESENTATION_TIMEOUT),
            Some(PresentationDeadline::Retry)
        );
        assert!(transaction.page_for_submission().is_some());
        assert!(transaction.document_submitted(first, now));
        assert_eq!(
            transaction.deadline(now + FIRST_PRESENTATION_TIMEOUT),
            Some(PresentationDeadline::Failed)
        );
        assert!(!transaction.is_loading());
    }

    #[test]
    fn first_presentation_releases_retry_bytes() {
        let mut transaction = NavigationTransaction::new(page());
        let document = transaction.document_id().unwrap();
        assert!(transaction.document_submitted(document, Instant::now()));
        assert!(transaction.mark_presented(document));
        assert!(transaction.page_for_submission().is_none());
        assert!(!transaction.is_loading());
    }
}
