//! Searchable open-tab and recently-closed popup owned by a browser window.

mod window;
pub(super) use window::window_proc;

use super::browser_app::BrowserApplication;
use super::paint_primitives::{draw_text_in_rect, fill_color_rect, fill_color_shape};
use super::tabs::TabId;
use super::*;
use std::rc::Rc;

const POPUP_WIDTH_DIP: i32 = 420;
const POPUP_MAX_HEIGHT_DIP: i32 = 700;
const POPUP_MIN_HEIGHT_DIP: i32 = 300;
const SEARCH_HEIGHT_DIP: i32 = 40;
const HEADER_HEIGHT_DIP: i32 = 64;
const SECTION_HEIGHT_DIP: i32 = 28;
const ROW_HEIGHT_DIP: i32 = 52;

#[derive(Clone)]
enum TabSearchEntryKind {
    Open { window: usize, id: TabId },
    Closed { id: u64 },
}

impl TabSearchEntryKind {
    fn section(&self) -> &'static str {
        match self {
            Self::Open { .. } => "Open tabs",
            Self::Closed { .. } => "Recently closed",
        }
    }
}

#[derive(Clone)]
struct TabSearchEntry {
    kind: TabSearchEntryKind,
    title: String,
    url: String,
}

struct TabSearchState {
    app: Rc<BrowserApplication>,
    owner: Hwnd,
    window: Hwnd,
    edit: Hwnd,
    dpi: u32,
    entries: Vec<TabSearchEntry>,
    filtered: Vec<usize>,
    selected: usize,
    first_visible: usize,
}

impl TabSearchState {
    unsafe fn new(app: Rc<BrowserApplication>, owner: Hwnd, dpi: u32) -> Self {
        let mut entries = Vec::new();
        for window in app.window_handles() {
            let Some(pointer) = app.state_pointer(window) else {
                continue;
            };
            entries.extend((*pointer).tabs.iter().map(|tab| TabSearchEntry {
                kind: TabSearchEntryKind::Open {
                    window: window as usize,
                    id: tab.id,
                },
                title: tab.title.clone(),
                url: tab.current_url().unwrap_or("New Tab").to_string(),
            }));
        }
        entries.extend(app.recent_closed_tabs().into_iter().map(|tab| {
            let url = tab.current_url().unwrap_or("New Tab").to_string();
            TabSearchEntry {
                kind: TabSearchEntryKind::Closed { id: tab.id },
                title: tab.title,
                url,
            }
        }));
        let filtered = (0..entries.len()).collect();
        Self {
            app,
            owner,
            window: null_mut(),
            edit: null_mut(),
            dpi,
            entries,
            filtered,
            selected: 0,
            first_visible: 0,
        }
    }

    fn scale(&self, value: i32) -> i32 {
        scale_dip(value, self.dpi)
    }

    unsafe fn refresh_filter(&mut self) {
        let query = window_text(self.edit).trim().to_lowercase();
        self.filtered = self
            .entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry_matches(entry, &query))
            .map(|(index, _)| index)
            .collect();
        self.selected = 0;
        self.first_visible = 0;
        InvalidateRect(self.window, null(), 0);
    }

    fn visible_rows(&self, client: Rect) -> Vec<(usize, Rect)> {
        let mut rows = Vec::new();
        let mut y = self.scale(HEADER_HEIGHT_DIP);
        let mut section = None;
        for position in self.first_visible..self.filtered.len() {
            let entry = &self.entries[self.filtered[position]];
            let next_section = entry.kind.section();
            if section != Some(next_section) {
                y += self.scale(SECTION_HEIGHT_DIP);
                section = Some(next_section);
            }
            let bottom = y + self.scale(ROW_HEIGHT_DIP);
            if bottom > client.bottom - self.scale(8) {
                break;
            }
            rows.push((
                position,
                Rect {
                    left: self.scale(8),
                    top: y,
                    right: client.right - self.scale(8),
                    bottom,
                },
            ));
            y = bottom;
        }
        rows
    }

    unsafe fn move_selection(&mut self, forward: bool) {
        if self.filtered.is_empty() {
            return;
        }
        self.selected = if forward {
            (self.selected + 1).min(self.filtered.len() - 1)
        } else {
            self.selected.saturating_sub(1)
        };
        if self.selected < self.first_visible {
            self.first_visible = self.selected;
        } else {
            let mut client: Rect = std::mem::zeroed();
            GetClientRect(self.window, &mut client);
            while self.first_visible < self.selected
                && !self
                    .visible_rows(client)
                    .iter()
                    .any(|(position, _)| *position == self.selected)
            {
                self.first_visible += 1;
            }
        }
        InvalidateRect(self.window, null(), 0);
    }

    unsafe fn activate_position(&mut self, position: usize) {
        let Some(entry_index) = self.filtered.get(position).copied() else {
            return;
        };
        let entry = self.entries[entry_index].clone();
        match entry.kind {
            TabSearchEntryKind::Open { window, id } => {
                if let Some(pointer) = self.app.state_pointer(window as Hwnd) {
                    (*pointer).activate_tab(id);
                    SetForegroundWindow(window as Hwnd);
                }
            }
            TabSearchEntryKind::Closed { id } => {
                if let Some(closed) = self.app.take_closed_tab(id)
                    && let Some(pointer) = self.app.state_pointer(self.owner)
                {
                    (*pointer).restore_closed_tab(closed);
                    SetForegroundWindow(self.owner);
                }
            }
        }
        PostMessageW(self.window, WM_CLOSE, 0, 0);
    }

    unsafe fn paint(&self) {
        let mut paint: PaintStruct = std::mem::zeroed();
        let dc = BeginPaint(self.window, &mut paint);
        if dc.is_null() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        fill_color_rect(dc, &client, CHROME_THEME.card);
        let owner = self.app.state_pointer(self.owner);
        let fonts = owner.and_then(|pointer| (*pointer).fonts.as_ref());
        let Some(fonts) = fonts else {
            EndPaint(self.window, &paint);
            return;
        };
        SetBkMode(dc, TRANSPARENT);
        let mut last_section = None;
        for (position, row) in self.visible_rows(client) {
            let entry = &self.entries[self.filtered[position]];
            let section = entry.kind.section();
            if last_section != Some(section) {
                SelectObject(dc, fonts.ui_semibold);
                SetTextColor(dc, CHROME_THEME.muted_text);
                let mut heading = Rect {
                    left: row.left + self.scale(8),
                    top: row.top - self.scale(SECTION_HEIGHT_DIP),
                    right: row.right,
                    bottom: row.top,
                };
                draw_text_in_rect(
                    dc,
                    section,
                    &mut heading,
                    DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
                );
                last_section = Some(section);
            }
            if position == self.selected {
                fill_color_shape(dc, &row, CHROME_THEME.accent_soft, self.scale(9) as f32);
            }
            let indicator = Rect {
                left: row.left + self.scale(10),
                top: row.top + self.scale(20),
                right: row.left + self.scale(20),
                bottom: row.top + self.scale(30),
            };
            fill_color_shape(dc, &indicator, CHROME_THEME.accent, self.scale(5) as f32);
            SelectObject(dc, fonts.ui_semibold);
            SetTextColor(dc, CHROME_THEME.text);
            let mut title = Rect {
                left: indicator.right + self.scale(10),
                top: row.top + self.scale(5),
                right: row.right - self.scale(10),
                bottom: row.top + self.scale(28),
            };
            draw_text_in_rect(
                dc,
                &entry.title,
                &mut title,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
            SelectObject(dc, fonts.ui_small);
            SetTextColor(dc, CHROME_THEME.muted_text);
            let mut url = Rect {
                left: title.left,
                top: row.top + self.scale(26),
                right: title.right,
                bottom: row.bottom - self.scale(4),
            };
            draw_text_in_rect(
                dc,
                &entry.url,
                &mut url,
                DT_VCENTER | DT_SINGLELINE | DT_END_ELLIPSIS | DT_NOPREFIX,
            );
        }
        if self.filtered.is_empty() {
            SelectObject(dc, fonts.ui);
            SetTextColor(dc, CHROME_THEME.muted_text);
            let mut empty = Rect {
                left: self.scale(20),
                top: self.scale(HEADER_HEIGHT_DIP + 24),
                right: client.right - self.scale(20),
                bottom: client.bottom,
            };
            draw_text_in_rect(
                dc,
                "No matching tabs",
                &mut empty,
                DT_CENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
        EndPaint(self.window, &paint);
    }
}

fn entry_matches(entry: &TabSearchEntry, query: &str) -> bool {
    query.is_empty()
        || entry.title.to_lowercase().contains(query)
        || entry.url.to_lowercase().contains(query)
}

#[cfg(test)]
#[path = "tab_search/tests.rs"]
mod tests;

impl BrowserState {
    pub(super) unsafe fn toggle_tab_search(&mut self) {
        if !self.tab_search_window.is_null() && IsWindow(self.tab_search_window) != 0 {
            DestroyWindow(self.tab_search_window);
            return;
        }
        let mut origin = Point {
            x: self.scale(4),
            y: self.scale(TAB_STRIP_HEIGHT_DIP),
        };
        ClientToScreen(self.window, &mut origin);
        let mut owner: Rect = std::mem::zeroed();
        GetWindowRect(self.window, &mut owner);
        let height = (owner.bottom - origin.y - self.scale(12)).clamp(
            self.scale(POPUP_MIN_HEIGHT_DIP),
            self.scale(POPUP_MAX_HEIGHT_DIP),
        );
        let width = self
            .scale(POPUP_WIDTH_DIP)
            .min((owner.width() - self.scale(16)).max(self.scale(280)));
        let state = Box::new(TabSearchState::new(
            Rc::clone(&self.app),
            self.window,
            self.dpi,
        ));
        let pointer = Box::into_raw(state);
        let class = wide(TAB_SEARCH_CLASS);
        let title = wide("Search tabs");
        let window = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class.as_ptr(),
            title.as_ptr(),
            WS_POPUP | WS_BORDER | WS_VISIBLE,
            origin.x,
            origin.y,
            width,
            height,
            self.window,
            null_mut(),
            self.instance,
            pointer.cast(),
        );
        if window.is_null() {
            // WM_NCDESTROY normally reclaims the state after a creation failure.
            // Avoid a double free if WM_CREATE created some, but not all, controls.
            self.set_status(&last_error("open tab search"));
            return;
        }
        self.tab_search_window = window;
        self.tab_search_edit = (*pointer).edit;
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        SetFocus(self.tab_search_edit);
    }

    pub(super) unsafe fn handle_tab_search_key(&mut self, key: usize) -> bool {
        if self.tab_search_window.is_null() || IsWindow(self.tab_search_window) == 0 {
            return false;
        }
        let pointer =
            GetWindowLongPtrW(self.tab_search_window, GWLP_USERDATA) as *mut TabSearchState;
        if pointer.is_null() {
            return false;
        }
        match key {
            VK_ESCAPE => PostMessageW(self.tab_search_window, WM_CLOSE, 0, 0) != 0,
            VK_UP => {
                (*pointer).move_selection(false);
                true
            }
            VK_DOWN => {
                (*pointer).move_selection(true);
                true
            }
            VK_RETURN => {
                let selected = (*pointer).selected;
                (*pointer).activate_position(selected);
                true
            }
            _ => false,
        }
    }
}
