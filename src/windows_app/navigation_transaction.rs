//! Per-tab authority for one full-document navigation and its renderer attempt.

use super::document_activation::LoadedPage;
use better_web_browser::renderer_protocol::DocumentId;
use std::time::{Duration, Instant};

pub(super) const FIRST_PRESENTATION_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_FIRST_PRESENTATION_RETRIES: u8 = 1;
const MAX_POST_PRESENTATION_RECOVERIES: u8 = 1;

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
    network_pending: bool,
    page: Option<LoadedPage>,
    document: Option<DocumentId>,
    first_presentation_deadline: Option<Instant>,
    retry_count: u8,
    post_presentation_recovery_count: u8,
}

impl NavigationTransaction {
    pub(super) fn new(page: LoadedPage) -> Self {
        Self {
            generation: 1,
            phase: NavigationPhase::WaitingForRenderer,
            network_pending: false,
            page: Some(page),
            document: None,
            first_presentation_deadline: None,
            retry_count: 0,
            post_presentation_recovery_count: 0,
        }
    }

    pub(super) fn begin(&mut self) -> u64 {
        self.post_presentation_recovery_count = 0;
        self.begin_fetch()
    }

    pub(super) fn begin_recovery(&mut self) -> Option<u64> {
        if self.phase != NavigationPhase::Presented
            || self.post_presentation_recovery_count >= MAX_POST_PRESENTATION_RECOVERIES
        {
            return None;
        }
        self.post_presentation_recovery_count += 1;
        Some(self.begin_fetch())
    }

    fn begin_fetch(&mut self) -> u64 {
        self.generation = self.generation.wrapping_add(1).max(1);
        self.phase = NavigationPhase::Fetching;
        self.network_pending = true;
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
        self.network_pending = page.stream.is_some();
        self.page = Some(page);
        self.phase = NavigationPhase::WaitingForRenderer;
        true
    }

    pub(super) fn page_for_submission(&self) -> Option<LoadedPage> {
        (self.phase == NavigationPhase::WaitingForRenderer)
            .then(|| self.page.clone())
            .flatten()
    }

    pub(super) fn network_complete(&mut self, bytes: u64, network_time: Duration) {
        self.network_pending = false;
        if self.phase == NavigationPhase::WaitingForFirstPresentation {
            self.first_presentation_deadline = Some(Instant::now() + FIRST_PRESENTATION_TIMEOUT);
        }
        if let Some(page) = self.page.as_mut() {
            page.bytes = bytes;
            page.network_time = network_time;
        }
    }

    pub(super) fn network_pending(&self) -> bool {
        self.network_pending
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
        self.first_presentation_deadline =
            (!self.network_pending).then_some(now + FIRST_PRESENTATION_TIMEOUT);
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
            self.network_pending = false;
            self.phase = NavigationPhase::Failed;
            self.page = None;
            PresentationDeadline::Failed
        }
    }

    pub(super) fn fail(&mut self) {
        self.network_pending = false;
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
        self.network_pending
            || matches!(
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
            stream: None,
            body: b"page".to_vec(),
            final_url: "https://example.test/".into(),
            status: 200,
            content_type: "text/html".into(),
            policy: Default::default(),
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

    #[test]
    fn slow_response_does_not_consume_the_renderer_deadline() {
        let mut transaction = NavigationTransaction::new(page());
        let generation = transaction.begin();
        let mut streaming = page();
        streaming.stream = Some(Default::default());
        assert!(transaction.accept_page(generation, streaming));
        let now = Instant::now();
        let document = transaction.document_id().unwrap();
        assert!(transaction.document_submitted(document, now));
        assert_eq!(transaction.deadline(now + Duration::from_secs(60)), None);
        assert!(transaction.is_loading());
        transaction.network_complete(400, Duration::from_secs(60));
        assert!(!transaction.network_pending());
        assert!(transaction.first_presentation_deadline.is_some());
        assert!(transaction.is_loading());
        assert!(transaction.mark_presented(document));
        assert!(!transaction.is_loading());
    }

    #[test]
    fn early_paint_keeps_loading_until_eof_and_failure_clears_pending_network() {
        let mut transaction = NavigationTransaction::new(page());
        let generation = transaction.begin();
        let mut streaming = page();
        streaming.stream = Some(Default::default());
        transaction.accept_page(generation, streaming);
        let document = transaction.document_id().unwrap();
        transaction.document_submitted(document, Instant::now());
        transaction.mark_presented(document);
        assert!(transaction.is_loading());
        transaction.network_complete(400, Duration::from_secs(2));
        assert!(!transaction.is_loading());
        transaction.begin();
        transaction.fail();
        assert!(!transaction.is_loading());
    }

    #[test]
    fn post_presentation_recovery_is_bounded_and_new_navigation_resets_it() {
        let mut transaction = NavigationTransaction::new(page());
        let document = transaction.document_id().unwrap();
        assert!(transaction.document_submitted(document, Instant::now()));
        assert!(transaction.mark_presented(document));
        assert!(transaction.begin_recovery().is_some());

        let recovered = transaction.document_id().unwrap();
        assert!(transaction.accept_page(transaction.generation(), page()));
        assert!(transaction.document_submitted(recovered, Instant::now()));
        assert!(transaction.mark_presented(recovered));
        assert_eq!(transaction.begin_recovery(), None);

        transaction.begin();
        let next = transaction.generation();
        assert!(transaction.accept_page(next, page()));
        let document = transaction.document_id().unwrap();
        assert!(transaction.document_submitted(document, Instant::now()));
        assert!(transaction.mark_presented(document));
        assert!(transaction.begin_recovery().is_some());
    }
}
