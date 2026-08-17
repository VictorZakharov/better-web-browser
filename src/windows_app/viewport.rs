//! Page/reader layout, scrolling, and content hit testing.

use super::browser_navigation::HistoryMode;
use super::paint_index::PaintIndex;
use super::tab_state::TabFocus;
use super::*;

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
        let layout_started = Instant::now();
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let dc = GetDC(self.window);
        if dc.is_null() {
            return DisplayListDamage::full(self.tabs.active().page_layout.items.len());
        }
        let dpi = self.dpi;
        let scale = self.page_scale();
        let viewport_height = self.viewport_height() as f32 / scale;
        let style_viewport_width = if self.media_viewport_width > 0.0 {
            self.media_viewport_width
        } else {
            client.right.max(1) as f32 / scale
        };
        let metrics = Arc::clone(&self.metrics);
        let fonts = self.fonts.as_ref();
        let content_margin = scale_dip(CONTENT_MARGIN_DIP, dpi);
        let available = (client.right - content_margin * 2).max(scale_dip(220, dpi));
        let reading_width = available.min(scale_dip(MAX_READING_WIDTH_DIP, dpi));
        SetBkMode(dc, TRANSPARENT);
        let mut damage = DisplayListDamage::full(self.tabs.active().page_layout.items.len());
        let tab = self.tabs.active_mut();
        tab.last_text_measure_count = 0;
        match tab.surface {
            Surface::Page => {
                let viewport_width = client.right.max(1) as f32 / scale;
                let mut measurer = GdiTextMeasurer {
                    dc,
                    fonts: &mut tab.dynamic_fonts,
                    dpi,
                    calls: 0,
                };
                let next_layout = layout_page_with_style_viewport(
                    &tab.page,
                    viewport_width,
                    viewport_height,
                    style_viewport_width,
                    &mut measurer,
                );
                let measure_calls = measurer.calls;
                damage = DisplayListDamage::between(&tab.page_layout, &next_layout);
                tab.page_layout = next_layout;
                tab.paint_index.rebuild(&tab.page_layout.items);
                tab.last_text_measure_count = measure_calls;
                tab.content_height = (tab.page_layout.content_height * scale).ceil() as i32;
                metrics.set_retained_draw_items(tab.page_layout.items.len());
            }
            Surface::Reader => {
                tab.paint_index = PaintIndex::default();
                let Some(fonts) = fonts else {
                    ReleaseDC(self.window, dc);
                    return damage;
                };
                let left = ((client.right - reading_width) / 2).max(content_margin);
                let Some(document) = tab.document.as_ref() else {
                    ReleaseDC(self.window, dc);
                    return damage;
                };
                let (items, height) = layout_document(dc, fonts, document, left, reading_width);
                tab.draw_items = items;
                tab.content_height = height;
                metrics.set_retained_draw_items(tab.draw_items.len());
            }
        }
        ReleaseDC(self.window, dc);
        let layout_tree_time = layout_started.elapsed();
        self.tabs.active_mut().last_layout_tree_time = layout_tree_time;
        self.clamp_scroll();
        self.update_scrollbar();
        self.recreate_page_controls();
        let layout_finalize_time = layout_started.elapsed().saturating_sub(layout_tree_time);
        let tab = self.tabs.active_mut();
        tab.last_layout_finalize_time = layout_finalize_time;
        tab.layout_dirty = false;
        damage
    }

    pub(super) unsafe fn toggle_reader(&mut self) {
        if self.surface == Surface::Page && self.document.is_none() {
            self.document = Some(parse_html(&self.reader_html, &self.reader_url));
        }
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
        self.scroll_y = position;
        self.clamp_scroll();
        self.update_scrollbar();
        self.sync_page_control_positions();
        InvalidateRect(self.window, null(), 0);
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
            Surface::Page => {
                let scale = self.page_scale();
                let document_x = x as f32 / scale;
                let document_y = (y - toolbar_height + self.scroll_y) as f32 / scale;
                self.page_layout.items.iter().find_map(|item| match item {
                    DisplayItem::Text {
                        rect,
                        link: Some(link),
                        ..
                    } if document_x >= rect.x
                        && document_x <= rect.right()
                        && document_y >= rect.y
                        && document_y <= rect.bottom() =>
                    {
                        Some(link.clone())
                    }
                    _ => None,
                })
            }
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
