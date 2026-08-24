//! Complete document-owned state retained independently for each browser tab.

use super::accessibility::AccessibilityDocument;
use super::document_navigation::ScriptNavigationGuard;
use super::page_controls::PageControlWindow;
use super::paint_index::PaintIndex;
use super::scrolling::ScrollAnimation;
use super::tabs::{IdentifiedTab, TabId};
use super::*;
use better_web_browser::engine::dom::NodeId;
use better_web_browser::fetch::FetchController;
use better_web_browser::renderer_process::RendererSession;
use better_web_browser::renderer_protocol::{DocumentId, PresentedGlyphRaster, ScrollInput};
use better_web_browser::storage::SessionStorage;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;

static NEXT_CLOSED_TAB_ID: AtomicU64 = AtomicU64::new(1);

pub(super) struct BrowserTab {
    pub(super) id: TabId,
    pub(super) title: String,
    pub(super) omnibox_text: String,
    pub(super) status_text: String,
    pub(super) performance: TabPerformance,
    pub(super) focus: TabFocus,
    pub(super) accessibility_document: AccessibilityDocument,
    pub(super) dynamic_fonts: DynamicFonts,
    pub(super) image_bitmaps: ImageBitmaps,
    pub(super) presented_images: HashMap<String, DecodedImage>,
    pub(super) glyph_bitmaps: GlyphBitmaps,
    pub(super) presented_glyphs: HashMap<u32, PresentedGlyphRaster>,
    pub(super) glyph_epoch: u64,
    pub(super) last_scroll_activity: Option<Instant>,
    pub(super) document: Option<Document>,
    pub(super) reader_url: String,
    pub(super) draw_items: Vec<DrawItem>,
    pub(super) page_layout: LayoutOutput,
    pub(super) paint_index: PaintIndex,
    pub(super) page_controls: Vec<PageControlWindow>,
    pub(super) surface: Surface,
    pub(super) content_height: i32,
    pub(super) scroll_y: i32,
    pub(super) scroll_animation: ScrollAnimation,
    pub(super) history: Vec<String>,
    pub(super) history_index: usize,
    pub(super) script_navigation: ScriptNavigationGuard,
    pub(super) generation: u64,
    pub(super) loading: bool,
    pub(super) crashed: bool,
    pub(super) document_fetch: FetchController,
    pub(super) session_storage: SessionStorage,
    pub(super) renderer_session: Option<RendererSession>,
    pub(super) renderer_launch_receiver: Option<mpsc::Receiver<Result<RendererSession, String>>>,
    pub(super) renderer_launch_pending: bool,
    pub(super) renderer_started_once: bool,
    pub(super) pending_renderer_page: Option<LoadedPage>,
    pub(super) renderer_document: Option<DocumentId>,
    pub(super) renderer_input_sequence: u64,
    pub(super) renderer_input_poll_budget: u8,
    pub(super) pending_renderer_scroll: Option<ScrollInput>,
    pub(super) renderer_revision: u64,
    pub(super) renderer_load_metrics: Option<RendererLoadMetrics>,
    pub(super) page_diagnostics: better_web_browser::renderer_protocol::PageDiagnostics,
    pub(super) renderer_next_timer: Option<Duration>,
    pub(super) renderer_runtime_clock: Option<Instant>,
    pub(super) renderer_work_pending: bool,
    pub(super) layout_dirty: bool,
    pub(super) render_dpi: u32,
}

impl BrowserTab {
    pub(super) fn new(id: TabId) -> Self {
        Self {
            id,
            title: "New Tab".into(),
            omnibox_text: String::new(),
            status_text: "Ready".into(),
            performance: TabPerformance::default(),
            focus: TabFocus::Address,
            accessibility_document: AccessibilityDocument::default(),
            dynamic_fonts: DynamicFonts::default(),
            image_bitmaps: ImageBitmaps::default(),
            presented_images: HashMap::new(),
            glyph_bitmaps: GlyphBitmaps::default(),
            presented_glyphs: HashMap::new(),
            glyph_epoch: 0,
            last_scroll_activity: None,
            document: None,
            reader_url: HOME_URL.into(),
            draw_items: Vec::new(),
            page_layout: LayoutOutput::default(),
            paint_index: PaintIndex::default(),
            page_controls: Vec::new(),
            surface: Surface::Page,
            content_height: 0,
            scroll_y: 0,
            scroll_animation: ScrollAnimation::default(),
            history: Vec::new(),
            history_index: 0,
            script_navigation: ScriptNavigationGuard::default(),
            generation: 1,
            loading: false,
            crashed: false,
            document_fetch: FetchController::new(),
            session_storage: SessionStorage::default(),
            renderer_session: None,
            renderer_launch_receiver: None,
            renderer_launch_pending: false,
            renderer_started_once: false,
            pending_renderer_page: Some(LoadedPage::home()),
            renderer_document: None,
            renderer_input_sequence: 0,
            renderer_input_poll_budget: 0,
            pending_renderer_scroll: None,
            renderer_revision: 0,
            renderer_load_metrics: None,
            page_diagnostics: Default::default(),
            renderer_next_timer: None,
            renderer_runtime_clock: None,
            renderer_work_pending: false,
            layout_dirty: true,
            render_dpi: DEFAULT_DPI,
        }
    }

    pub(super) fn current_url(&self) -> Option<&str> {
        self.history
            .get(self.history_index)
            .map(String::as_str)
            .filter(|url| !url.is_empty())
    }

    pub(super) fn mark_crashed(&mut self, status: String) {
        self.generation = self.generation.wrapping_add(1);
        self.loading = false;
        self.crashed = true;
        self.status_text = status;
        self.document_fetch.abort();
        self.pending_renderer_page = None;
        self.renderer_document = None;
        self.renderer_input_sequence = 0;
        self.renderer_input_poll_budget = 0;
        self.pending_renderer_scroll = None;
        self.renderer_revision = 0;
        self.renderer_load_metrics = None;
        self.page_diagnostics = Default::default();
        self.accessibility_document.clear();
        self.renderer_next_timer = None;
        self.renderer_runtime_clock = None;
        self.renderer_work_pending = false;
        self.page_controls.clear();
        if let Some(session) = self.renderer_session.take() {
            session.terminate_in_background();
        }
        self.renderer_launch_receiver.take();
        self.renderer_launch_pending = false;
    }
}

impl IdentifiedTab for BrowserTab {
    fn tab_id(&self) -> TabId {
        self.id
    }
}

impl Drop for BrowserTab {
    fn drop(&mut self) {
        self.page_controls.clear();
        self.document_fetch.abort();
        if let Some(session) = self.renderer_session.take() {
            session.terminate_in_background();
        }
        self.renderer_launch_receiver.take();
    }
}

#[derive(Clone, Copy)]
pub(super) enum TabFocus {
    Content,
    Address,
    PageControl(NodeId),
}

#[derive(Clone)]
pub(super) struct ClosedTab {
    pub(super) id: u64,
    pub(super) title: String,
    pub(super) history: Vec<String>,
    pub(super) history_index: usize,
}

impl From<&BrowserTab> for ClosedTab {
    fn from(tab: &BrowserTab) -> Self {
        Self {
            id: NEXT_CLOSED_TAB_ID.fetch_add(1, Ordering::Relaxed),
            title: tab.title.clone(),
            history: tab.history.clone(),
            history_index: tab.history_index,
        }
    }
}

impl ClosedTab {
    pub(super) fn current_url(&self) -> Option<&str> {
        self.history
            .get(self.history_index)
            .map(String::as_str)
            .filter(|url| !url.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::windows_app::tabs::TabCollection;

    #[test]
    fn new_tabs_queue_home_for_the_renderer_without_a_browser_document() {
        let mut tab = BrowserTab::new(TabId::first());
        assert!(tab.document.is_none());
        assert_eq!(tab.generation, 1);
        let home = tab
            .pending_renderer_page
            .take()
            .expect("home document is queued for the renderer");
        assert_eq!(home.final_url, HOME_URL);
        assert_eq!(home.content_type, "text/html");
        assert_eq!(home.body, HOME_HTML.as_bytes());
    }

    #[test]
    fn live_document_state_is_independent_between_tabs() {
        let mut tabs = TabCollection::new(BrowserTab::new(TabId::first()));
        tabs.active_mut()
            .history
            .push("https://first.example/".into());
        tabs.active_mut().scroll_y = 420;
        tabs.active_mut().title = "First".into();
        tabs.active_mut().loading = true;
        tabs.active_mut().focus = TabFocus::Content;
        let second = tabs.add(true, BrowserTab::new).unwrap();
        tabs.active_mut()
            .history
            .push("https://second.example/".into());
        tabs.active_mut().scroll_y = 17;

        tabs.activate(TabId::first());
        assert_eq!(tabs.active().current_url(), Some("https://first.example/"));
        assert_eq!(tabs.active().scroll_y, 420);
        assert_eq!(tabs.active().title, "First");
        assert!(tabs.active().loading);
        assert!(matches!(tabs.active().focus, TabFocus::Content));
        tabs.activate(second);
        assert_eq!(tabs.active().current_url(), Some("https://second.example/"));
        assert_eq!(tabs.active().scroll_y, 17);
    }

    #[test]
    fn crashing_one_tab_cancels_only_its_page_state() {
        let mut tabs = TabCollection::new(BrowserTab::new(TabId::first()));
        tabs.active_mut().loading = true;
        let first_document = better_web_browser::renderer_protocol::DocumentId::new(31).unwrap();
        let first_root =
            better_web_browser::renderer_protocol::DocumentNodeId::new((31_u128 << 64) | 1)
                .unwrap();
        tabs.active_mut()
            .accessibility_document
            .apply(
                first_document,
                1,
                better_web_browser::renderer_protocol::AccessibilityUpdate::full_root(
                    first_root,
                    "first",
                    better_web_browser::engine::RectF::default(),
                ),
            )
            .unwrap();
        let sibling = tabs.add(true, BrowserTab::new).unwrap();
        let sibling_document = better_web_browser::renderer_protocol::DocumentId::new(32).unwrap();
        let sibling_root =
            better_web_browser::renderer_protocol::DocumentNodeId::new((32_u128 << 64) | 1)
                .unwrap();
        tabs.active_mut()
            .accessibility_document
            .apply(
                sibling_document,
                1,
                better_web_browser::renderer_protocol::AccessibilityUpdate::full_root(
                    sibling_root,
                    "sibling",
                    better_web_browser::engine::RectF::default(),
                ),
            )
            .unwrap();
        tabs.active_mut()
            .history
            .push("https://sibling.example/".into());

        tabs.activate(TabId::first());
        tabs.active_mut()
            .mark_crashed("Renderer crashed. Reload to try again.".into());

        assert!(tabs.active().crashed);
        assert!(!tabs.active().loading);
        assert!(tabs.active().status_text.contains("Reload"));
        assert!(tabs.active().accessibility_document.root().is_none());
        tabs.activate(sibling);
        assert!(!tabs.active().crashed);
        assert_eq!(
            tabs.active().current_url(),
            Some("https://sibling.example/")
        );
        assert_eq!(
            tabs.active().accessibility_document.root(),
            Some(sibling_root)
        );
    }
}
