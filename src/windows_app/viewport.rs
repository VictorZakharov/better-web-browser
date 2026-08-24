//! Page/reader layout, scrolling, and content hit testing.

use super::browser_navigation::HistoryMode;
use super::paint_index::PaintIndex;
use super::tab_state::TabFocus;
use super::*;

const SW_INVALIDATE: u32 = 0x0002;

pub(super) struct DrawItem {
    pub(super) x: i32,
    pub(super) y: i32,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) text: String,
    pub(super) link: Option<String>,
    pub(super) font: FontKind,
    pub(super) color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Surface {
    Page,
    Reader,
}

impl BrowserState {
    pub(super) unsafe fn rebuild_layout(&mut self) -> DisplayListDamage {
        if self.surface == Surface::Page {
            let Some(document) = self.renderer_document else {
                // The pending document carries the current viewport when it enters the renderer.
                // There is intentionally no privileged in-process page-engine fallback.
                self.layout_dirty = false;
                return DisplayListDamage::default();
            };
            let viewport = self.renderer_viewport();
            let result = self
                .renderer_session
                .as_ref()
                .ok_or_else(|| "renderer session is unavailable".to_string())
                .and_then(|session| session.update_viewport(document, viewport));
            self.layout_dirty = false;
            self.renderer_work_pending = result.is_ok();
            if let Err(error) = result {
                self.contain_page_engine_failure(
                    self.id,
                    format!("could not resize the isolated document: {error}"),
                );
            }
            return DisplayListDamage::default();
        }

        // Reader extraction happens in the renderer. The browser lays out only that bounded,
        // validated semantic projection with trusted UI fonts.
        let layout_started = Instant::now();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let dc = GetDC(self.window);
        if dc.is_null() {
            return DisplayListDamage::full(self.draw_items.len());
        }
        let content_margin = scale_dip(CONTENT_MARGIN_DIP, self.dpi);
        let available = (client.right - content_margin * 2).max(scale_dip(220, self.dpi));
        let reading_width = available.min(scale_dip(MAX_READING_WIDTH_DIP, self.dpi));
        let left = ((client.right - reading_width) / 2).max(content_margin);
        let damage = DisplayListDamage::full(self.draw_items.len());
        self.paint_index = PaintIndex::default();
        if let (Some(fonts), Some(document)) = (self.fonts.as_ref(), self.document.as_ref()) {
            let (items, height) = layout_document(dc, fonts, document, left, reading_width);
            self.draw_items = items;
            self.content_height = height;
            self.metrics.set_retained_draw_items(self.draw_items.len());
        }
        ReleaseDC(self.window, dc);
        self.layout_dirty = false;
        self.clamp_scroll();
        self.update_scrollbar();
        self.destroy_page_controls();
        self.record_benchmark_layout(layout_started.elapsed(), damage);
        damage
    }

    pub(super) unsafe fn toggle_reader(&mut self) {
        self.surface = match self.surface {
            Surface::Page => Surface::Reader,
            Surface::Reader => Surface::Page,
        };
        set_window_text(
            self.controls.reader,
            if self.surface == Surface::Reader {
                "Page"
            } else {
                "Reader"
            },
        );
        InvalidateRect(self.controls.reader, null(), 0);
        self.scroll_y = 0;
        self.rebuild_layout();
        self.refresh_accessibility_full();
        InvalidateRect(self.window, null(), 0);
    }

    pub(super) unsafe fn viewport_height(&self) -> i32 {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        (client.bottom - self.toolbar_height() - self.status_height()).max(1)
    }

    pub(super) unsafe fn clamp_scroll(&mut self) {
        let max_scroll = (self.content_height - self.viewport_height()).max(0);
        self.scroll_y = self.scroll_y.clamp(0, max_scroll);
    }

    pub(super) unsafe fn scroll_to(&mut self, position: i32) {
        self.cancel_scroll_animation();
        self.commit_scroll_position(position);
    }

    pub(super) unsafe fn commit_scroll_position(&mut self, position: i32) {
        self.note_scroll_activity();
        let previous = self.scroll_y;
        self.scroll_y = position;
        self.clamp_scroll();
        if self.scroll_y == previous {
            return;
        }
        self.update_scrollbar();
        if !self.processing_background_tab {
            if self
                .benchmark
                .as_ref()
                .is_some_and(|benchmark| benchmark.early_scroll.is_some())
            {
                self.sync_page_control_positions();
                // An invisible window has no paintable update region. Exercise the same retained
                // display list through a bounded offscreen surface for the hidden trace.
                if let Err(error) = self.paint_benchmark_frame()
                    && let Some(benchmark) = self.benchmark.as_mut()
                {
                    benchmark
                        .error
                        .get_or_insert_with(|| format!("early-scroll paint failed: {error}"));
                }
            } else {
                let mut client: Rect = std::mem::zeroed();
                GetClientRect(self.window, &mut client);
                let content = Rect {
                    left: 0,
                    top: self.toolbar_height(),
                    right: client.right,
                    bottom: (client.bottom - self.status_height()).max(self.toolbar_height()),
                };
                let delta = previous - self.scroll_y;
                if delta.unsigned_abs() < content.height() as u32 {
                    // Preserve pixels that remain visible and invalidate only the exposed strip.
                    // <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-scrollwindowex>
                    ScrollWindowEx(
                        self.window,
                        0,
                        delta,
                        &content,
                        &content,
                        null_mut(),
                        null_mut(),
                        SW_INVALIDATE,
                    );
                } else {
                    InvalidateRect(self.window, &content, 0);
                }
                // Move native form controls after shifting the parent's retained pixels. Moving
                // them first lets their old/new invalidations participate in the bit scroll and
                // can leave a stale control-shaped patch behind.
                self.sync_page_control_positions();
                // WM_PAINT is low priority. Commit this invalidated scroll frame before the next
                // post-load task can occupy the UI thread.
                // <https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-updatewindow>
                UpdateWindow(self.window);
            }
        } else {
            self.sync_page_control_positions();
        }
        self.route_renderer_scroll();
        if !self.processing_background_tab {
            self.refresh_accessibility_document_bounds();
        }
    }

    pub(super) unsafe fn update_scrollbar(&self) {
        if self.processing_background_tab {
            return;
        }
        let info = ScrollInfo {
            size: size_of::<ScrollInfo>() as u32,
            mask: SIF_RANGE | SIF_PAGE | SIF_POS,
            min: 0,
            max: self.content_height.saturating_sub(1),
            page: self.viewport_height() as u32,
            position: self.scroll_y,
            track_position: 0,
        };
        SetScrollInfo(self.window, SB_VERT, &info, 1);
    }

    pub(super) unsafe fn handle_scroll(&mut self, command: u16) {
        let viewport = self.viewport_height();
        let target = match command {
            SB_LINEUP => self.scroll_y - 42,
            SB_LINEDOWN => self.scroll_y + 42,
            SB_PAGEUP => self.scroll_y - viewport,
            SB_PAGEDOWN => self.scroll_y + viewport,
            SB_TOP => 0,
            SB_BOTTOM => self.content_height,
            SB_THUMBPOSITION | SB_THUMBTRACK => {
                let mut info = ScrollInfo {
                    size: size_of::<ScrollInfo>() as u32,
                    mask: SIF_TRACKPOS,
                    min: 0,
                    max: 0,
                    page: 0,
                    position: 0,
                    track_position: 0,
                };
                GetScrollInfo(self.window, SB_VERT, &mut info);
                info.track_position
            }
            _ => self.scroll_y,
        };
        self.scroll_to(target);
    }

    pub(super) unsafe fn click_content(&mut self, x: i32, y: i32, background_tab: bool) {
        self.focus = TabFocus::Content;
        let toolbar_height = self.toolbar_height();
        if y < toolbar_height || y > toolbar_height + self.viewport_height() {
            return;
        }
        let url = match self.surface {
            Surface::Page => None,
            Surface::Reader => {
                let document_y = y - toolbar_height + self.scroll_y;
                self.draw_items
                    .iter()
                    .find(|item| {
                        item.link.is_some()
                            && x >= item.x
                            && x <= item.x + item.width
                            && document_y >= item.y
                            && document_y <= item.y + item.height
                    })
                    .and_then(|item| item.link.clone())
            }
        };
        if let Some(url) = url {
            if background_tab {
                self.open_url_in_new_tab(url, false);
            } else {
                self.begin_navigation(url, HistoryMode::Push);
            }
        }
    }
}
