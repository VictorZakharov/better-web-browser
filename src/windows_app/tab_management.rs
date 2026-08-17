//! Browser-window tab lifecycle, command routing, and deferred background work.

use super::tab_state::{BrowserTab, ClosedTab, TabFocus};
use super::tabs::{self, BrowserShortcut, IdentifiedTab, TabId, TabStripHit, TabStripLayout};
use super::*;
impl BrowserState {
    pub(super) fn tab_strip_layout(&self, client_width: i32) -> TabStripLayout {
        let ids = self
            .tabs
            .iter()
            .map(IdentifiedTab::tab_id)
            .collect::<Vec<_>>();
        TabStripLayout::calculate(client_width, self.dpi, &ids)
    }

    pub(super) unsafe fn new_tab(&mut self) {
        self.add_tab(None, true);
    }

    pub(super) unsafe fn open_url_in_new_tab(&mut self, url: String, foreground: bool) {
        self.add_tab(Some(url), foreground);
    }

    unsafe fn add_tab(&mut self, url: Option<String>, foreground: bool) {
        if foreground {
            self.suspend_active_tab_ui();
        }
        let added = self.tabs.add(foreground, BrowserTab::new);
        let id = match added {
            Ok(id) => id,
            Err(_) => {
                if foreground {
                    self.restore_active_tab_ui();
                }
                self.set_status(&format!("At most {} tabs can be open", tabs::MAX_OPEN_TABS));
                return;
            }
        };
        self.app.tab_router.bind(id, self.window);
        self.register_renderer_tab(id);
        self.start_renderer_for(id);
        if foreground {
            self.restore_active_tab_ui();
            SetFocus(self.controls.address);
        }
        if let Some(url) = url {
            self.begin_navigation_for_tab(id, url, browser_navigation::HistoryMode::Push);
        }
        InvalidateRect(self.window, null(), 0);
    }

    pub(super) unsafe fn activate_tab(&mut self, id: TabId) {
        if self.tabs.active_id() == id || !self.tabs.contains(id) {
            return;
        }
        self.suspend_active_tab_ui();
        self.tabs.activate_exclusive(id);
        self.restore_active_tab_ui();
    }

    pub(super) unsafe fn activate_relative_tab(&mut self, forward: bool) {
        if self.tabs.len() < 2 {
            return;
        }
        self.suspend_active_tab_ui();
        self.tabs.activate_relative(forward);
        self.restore_active_tab_ui();
    }

    pub(super) unsafe fn activate_tab_position(&mut self, one_based: usize) {
        if one_based == 0 || one_based > self.tabs.len() {
            return;
        }
        self.suspend_active_tab_ui();
        self.tabs.activate_position(one_based);
        self.restore_active_tab_ui();
    }

    pub(super) unsafe fn activate_last_tab(&mut self) {
        if self.tabs.active_index() == self.tabs.len() - 1 {
            return;
        }
        self.suspend_active_tab_ui();
        self.tabs.activate_last();
        self.restore_active_tab_ui();
    }

    pub(super) unsafe fn close_tab(&mut self, id: TabId) {
        if !self.tabs.contains(id) {
            return;
        }
        if self.tabs.active_id() != id {
            if let Some(tab) = self.tabs.remove(id) {
                self.app.tab_router.unbind(tab.id);
                self.remove_renderer_tab(tab.id);
                self.remember_closed_tab(ClosedTab::from(&tab));
                drop(tab);
                InvalidateRect(self.window, null(), 0);
            }
            return;
        }
        self.suspend_active_tab_ui();
        let tab = if self.tabs.len() == 1 {
            let (replacement_id, removed) = self.tabs.replace_active(BrowserTab::new);
            self.app.tab_router.bind(replacement_id, self.window);
            self.register_renderer_tab(replacement_id);
            removed
        } else {
            self.tabs.remove_active()
        };
        self.app.tab_router.unbind(tab.id);
        self.remove_renderer_tab(tab.id);
        self.remember_closed_tab(ClosedTab::from(&tab));
        drop(tab);
        self.start_renderer_for(self.tabs.active_id());
        self.restore_active_tab_ui();
    }

    pub(super) unsafe fn reopen_closed_tab(&mut self) {
        let Some(closed) = self.app.pop_closed_tab() else {
            return;
        };
        self.restore_closed_tab(closed);
    }

    pub(super) unsafe fn restore_closed_tab(&mut self, closed: ClosedTab) {
        self.suspend_active_tab_ui();
        let added = self.tabs.add(true, BrowserTab::new);
        let id = match added {
            Ok(id) => id,
            Err(_) => {
                self.app.remember_closed_tab(closed);
                self.restore_active_tab_ui();
                self.set_status(&format!("At most {} tabs can be open", tabs::MAX_OPEN_TABS));
                return;
            }
        };
        self.app.tab_router.bind(id, self.window);
        let url = {
            let tab = self.tabs.active_mut();
            tab.title = closed.title;
            tab.history = closed.history;
            tab.history_index = closed
                .history_index
                .min(tab.history.len().saturating_sub(1));
            tab.current_url().map(str::to_owned)
        };
        self.register_renderer_tab(id);
        self.start_renderer_for(id);
        self.restore_active_tab_ui();
        if let Some(url) = url {
            self.begin_navigation_for_tab(id, url, browser_navigation::HistoryMode::Existing);
        }
    }

    fn remember_closed_tab(&mut self, closed: ClosedTab) {
        self.app.remember_closed_tab(closed);
    }

    pub(super) unsafe fn suspend_active_tab_ui(&mut self) {
        self.omnibox_text = window_text(self.controls.address);
        KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        let focused = GetFocus();
        self.focus = if focused == self.controls.address {
            TabFocus::Address
        } else if let Some(control) = self
            .page_controls
            .iter()
            .find(|control| control.window == focused)
        {
            TabFocus::PageControl(control.spec.node_id)
        } else {
            TabFocus::Content
        };
        if matches!(self.focus, TabFocus::PageControl(_)) {
            SetFocus(self.window);
        }
        for control in &self.page_controls {
            ShowWindow(control.window, SW_HIDE);
        }
    }

    pub(super) unsafe fn restore_active_tab_ui(&mut self) {
        set_window_text(self.controls.address, &self.omnibox_text);
        set_window_text(
            self.controls.reader,
            if self.surface == Surface::Reader {
                "Page"
            } else {
                "Reader"
            },
        );
        self.update_history_buttons();
        if self.render_dpi != self.dpi {
            self.dynamic_fonts.clear();
            self.render_dpi = self.dpi;
            self.layout_dirty = true;
        }
        if self.layout_dirty {
            self.rebuild_layout();
        } else {
            self.clamp_scroll();
            self.update_scrollbar();
            self.sync_page_control_positions();
        }
        self.update_window_and_tab_title();
        self.resume_script_runtime();
        match self.focus {
            TabFocus::Address => {
                SetFocus(self.controls.address);
            }
            TabFocus::PageControl(node_id) => {
                if let Some(control) = self
                    .page_controls
                    .iter()
                    .find(|control| control.spec.node_id == node_id)
                {
                    SetFocus(control.window);
                } else {
                    self.focus = TabFocus::Content;
                    SetFocus(self.window);
                }
            }
            TabFocus::Content => {
                SetFocus(self.window);
            }
        }
        InvalidateRect(self.window, null(), 0);
    }

    pub(super) unsafe fn handle_shortcut(&mut self, shortcut: BrowserShortcut) {
        match shortcut {
            BrowserShortcut::NewWindow => self.open_browser_window(),
            BrowserShortcut::NewTab => self.new_tab(),
            BrowserShortcut::CloseWindow => {
                PostMessageW(self.window, WM_CLOSE, 0, 0);
            }
            BrowserShortcut::CloseTab => self.close_tab(self.tabs.active_id()),
            BrowserShortcut::ReopenClosedTab => self.reopen_closed_tab(),
            BrowserShortcut::SearchTabs => self.toggle_tab_search(),
            BrowserShortcut::NextTab => self.activate_relative_tab(true),
            BrowserShortcut::PreviousTab => self.activate_relative_tab(false),
            BrowserShortcut::MoveTabsLeft => self.move_selected_tabs(false),
            BrowserShortcut::MoveTabsRight => self.move_selected_tabs(true),
            BrowserShortcut::ActivatePosition(position) => self.activate_tab_position(position),
            BrowserShortcut::ActivateLast => self.activate_last_tab(),
            BrowserShortcut::FocusAddress => {
                self.focus = TabFocus::Address;
                SetFocus(self.controls.address);
                SendMessageW(self.controls.address, EM_SETSEL, 0, -1);
            }
            BrowserShortcut::Reload => self.reload(),
            BrowserShortcut::Back => self.go_back(),
            BrowserShortcut::Forward => self.go_forward(),
        }
    }

    pub(super) unsafe fn handle_tab_strip_click(&mut self, x: i32, y: i32) -> bool {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let Some(hit) = self.tab_strip_layout(client.right).hit_test(x, y) else {
            return false;
        };
        match hit {
            TabStripHit::Activate(id) => self.activate_tab(id),
            TabStripHit::Close(id) => self.close_tab(id),
            TabStripHit::NewTab => self.new_tab(),
            TabStripHit::SearchTabs => self.toggle_tab_search(),
        }
        true
    }

    pub(super) fn mark_all_tab_layouts_dirty(&mut self) {
        for tab in self.tabs.iter_mut() {
            tab.layout_dirty = true;
        }
    }

    pub(super) unsafe fn update_active_tab_title(&mut self, title: &str) {
        self.title.clear();
        self.title.push_str(if title.trim().is_empty() {
            "New Tab"
        } else {
            title
        });
        self.update_renderer_tab_title(self.id, &self.title);
        if !self.processing_background_tab {
            self.update_window_and_tab_title();
        }
    }

    unsafe fn update_window_and_tab_title(&self) {
        set_window_text(
            self.window,
            &format!("{} \u{2014} {PRODUCT_NAME}", self.title),
        );
        let strip = Rect {
            left: 0,
            top: 0,
            right: self.chrome.status.right,
            bottom: self.scale(TAB_STRIP_HEIGHT_DIP),
        };
        InvalidateRect(self.window, &strip, 0);
    }

    pub(super) unsafe fn route_navigation_message(&mut self, id: TabId, message: LoadMessage) {
        self.process_for_tab(id, |state| state.finish_navigation(message));
    }

    pub(super) unsafe fn route_resource_message(
        &mut self,
        id: TabId,
        message: DeferredResourcesMessage,
    ) {
        self.process_for_tab(id, |state| state.finish_deferred_resources(message));
    }

    pub(super) unsafe fn route_async_script_message(
        &mut self,
        id: TabId,
        message: async_scripts::AsyncScriptMessage,
    ) {
        self.process_for_tab(id, |state| state.finish_async_script(message));
    }

    // Boa realms and GDI layout remain UI-thread-owned. Select a background tab
    // internally to reuse the normal commit path, suppress its shared UI, then
    // restore the visible tab before Windows can dispatch another message.
    unsafe fn process_for_tab(&mut self, id: TabId, process: impl FnOnce(&mut Self)) {
        if !self.tabs.contains(id) {
            return;
        }
        let original = self.tabs.active_id();
        if original == id {
            process(self);
            return;
        }
        self.tabs.activate(id);
        self.processing_background_tab = true;
        process(self);
        KillTimer(self.window, ID_SCRIPT_RUNTIME_TIMER);
        for control in &self.page_controls {
            ShowWindow(control.window, SW_HIDE);
        }
        self.tabs.activate(original);
        self.processing_background_tab = false;
        let retained_items = match self.surface {
            Surface::Page => self.page_layout.items.len(),
            Surface::Reader => self.draw_items.len(),
        };
        self.metrics.set_retained_draw_items(retained_items);
        self.update_scrollbar();
        self.resume_script_runtime();
        let strip = Rect {
            left: 0,
            top: 0,
            right: self.chrome.status.right,
            bottom: self.scale(TAB_STRIP_HEIGHT_DIP),
        };
        InvalidateRect(self.window, &strip, 0);
    }
}
