//! Complete document-owned state retained independently for each browser tab.

use super::document_navigation::ScriptNavigationGuard;
use super::page_controls::PageControlWindow;
use super::paint_index::PaintIndex;
use super::scrolling::ScrollAnimation;
use super::tabs::{IdentifiedTab, TabId};
use super::*;
use better_web_browser::engine::dom::NodeId;
use better_web_browser::fetch::FetchController;
use better_web_browser::renderer_process::RendererSession;
use std::collections::VecDeque;
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
    pub(super) dynamic_fonts: DynamicFonts,
    pub(super) web_fonts: WebFontResources,
    pub(super) image_bitmaps: ImageBitmaps,
    pub(super) page: Page,
    pub(super) script_runtime: Option<ScriptRuntime>,
    pub(super) script_runtime_clock: Option<Instant>,
    pub(super) post_load_script_not_before: Option<Instant>,
    pub(super) pending_async_scripts: VecDeque<async_scripts::AsyncScriptMessage>,
    pub(super) last_scroll_activity: Option<Instant>,
    pub(super) loaded_page_resources: HashSet<PageResource>,
    pub(super) page_resource_budget: u64,
    pub(super) document: Option<Document>,
    pub(super) reader_html: String,
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
    pub(super) renderer_session: Option<RendererSession>,
    pub(super) renderer_launch_receiver: Option<mpsc::Receiver<Result<RendererSession, String>>>,
    pub(super) renderer_launch_pending: bool,
    pub(super) renderer_started_once: bool,
    pub(super) last_layout_tree_time: Duration,
    pub(super) last_layout_finalize_time: Duration,
    pub(super) last_text_measure_count: usize,
    pub(super) layout_dirty: bool,
    pub(super) render_dpi: u32,
}

impl BrowserTab {
    pub(super) fn new(id: TabId) -> Self {
        let document = parse_html(HOME_HTML, HOME_URL);
        let page = Page::parse(HOME_HTML, HOME_URL);
        Self {
            id,
            title: "New Tab".into(),
            omnibox_text: String::new(),
            status_text: "Ready".into(),
            performance: TabPerformance::default(),
            focus: TabFocus::Address,
            dynamic_fonts: DynamicFonts::default(),
            web_fonts: WebFontResources::default(),
            image_bitmaps: ImageBitmaps::default(),
            page,
            script_runtime: None,
            script_runtime_clock: None,
            post_load_script_not_before: None,
            pending_async_scripts: VecDeque::new(),
            last_scroll_activity: None,
            loaded_page_resources: HashSet::new(),
            page_resource_budget: PAGE_RESOURCE_BUDGET,
            document: Some(document),
            reader_html: HOME_HTML.into(),
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
            generation: 0,
            loading: false,
            crashed: false,
            document_fetch: FetchController::new(),
            renderer_session: None,
            renderer_launch_receiver: None,
            renderer_launch_pending: false,
            renderer_started_once: false,
            last_layout_tree_time: Duration::ZERO,
            last_layout_finalize_time: Duration::ZERO,
            last_text_measure_count: 0,
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
        if let Some(mut runtime) = self.script_runtime.take() {
            runtime.cancel_document();
        }
        self.renderer_session.take();
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
}
