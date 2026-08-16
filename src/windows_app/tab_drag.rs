//! Tab-strip selection, drag reordering, and cross-window transfer.

use super::browser_window::{BrowserWindowPlacement, create_browser_window};
use super::tab_state::BrowserTab;
use super::tabs::{KeyModifiers, TabId, TabStripHit};
use super::*;
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TabDropTarget {
    window: usize,
    index: usize,
}

pub(super) struct TabDragGesture {
    pressed: TabId,
    start: Point,
    dragging: bool,
    collapse_on_release: bool,
    target: Option<TabDropTarget>,
}

impl BrowserApplication {
    unsafe fn tab_drop_target(&self, screen: Point) -> Option<TabDropTarget> {
        let under_cursor = WindowFromPoint(screen);
        if under_cursor.is_null() {
            return None;
        }
        let root = GetAncestor(under_cursor, GA_ROOT);
        if root.is_null() || !self.window_handles().contains(&root) {
            return None;
        }
        let state = self.state_pointer(root)?;
        let mut client = screen;
        if ScreenToClient(root, &mut client) == 0 {
            return None;
        }
        let mut bounds: Rect = std::mem::zeroed();
        GetClientRect(root, &mut bounds);
        let layout = (*state).tab_strip_layout(bounds.right);
        layout.contains_strip_y(client.y).then_some(TabDropTarget {
            window: root as usize,
            index: layout.insertion_index(client.x),
        })
    }
}

impl BrowserState {
    pub(super) unsafe fn open_browser_window(&mut self) {
        let mut source_bounds: Rect = std::mem::zeroed();
        GetWindowRect(self.window, &mut source_bounds);
        let state = BrowserState::detached_placeholder(Rc::clone(&self.app));
        let placement = BrowserWindowPlacement::offset(source_bounds, self.dpi);
        let window = match create_browser_window(state, placement) {
            Ok(window) => window,
            Err(error) => {
                self.set_status(&error);
                return;
            }
        };
        let Some(pointer) = self.app.state_pointer(window) else {
            DestroyWindow(window);
            self.set_status("New window did not retain its browser state");
            return;
        };
        (*pointer).complete_detached_startup();
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        SetForegroundWindow(window);
    }

    pub(super) unsafe fn begin_tab_pointer(
        &mut self,
        point: Point,
        modifiers: KeyModifiers,
    ) -> bool {
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let Some(TabStripHit::Activate(id)) = self
            .tab_strip_layout(client.right)
            .hit_test(point.x, point.y)
        else {
            return false;
        };

        let collapse_on_release = !modifiers.control
            && !modifiers.shift
            && self.tabs.is_selected(id)
            && self.tabs.selection_len() > 1;
        self.suspend_active_tab_ui();
        if modifiers.shift {
            self.tabs.select_range(id);
        } else if modifiers.control {
            self.tabs.toggle_selection(id);
        } else if collapse_on_release {
            self.tabs.activate(id);
        } else {
            self.tabs.activate_exclusive(id);
        }
        self.restore_active_tab_ui();
        self.tab_drag = Some(TabDragGesture {
            pressed: id,
            start: point,
            dragging: false,
            collapse_on_release,
            target: None,
        });
        // Keep receiving pointer messages while the cursor crosses top-level
        // windows. Ownership moves only after release, once the drop is known.
        SetCapture(self.window);
        true
    }

    pub(super) unsafe fn update_tab_pointer(&mut self, point: Point) -> bool {
        let Some(mut gesture) = self.tab_drag.take() else {
            return false;
        };
        let threshold = self.scale(5).max(3);
        if !gesture.dragging
            && (point.x - gesture.start.x).abs() < threshold
            && (point.y - gesture.start.y).abs() < threshold
        {
            self.tab_drag = Some(gesture);
            return true;
        }
        gesture.dragging = true;
        gesture.collapse_on_release = false;
        let mut screen = point;
        ClientToScreen(self.window, &mut screen);
        let target = self.app.tab_drop_target(screen).filter(|target| {
            target.window == self.window as usize
                || self
                    .app
                    .state_pointer(target.window as Hwnd)
                    .is_some_and(|state| {
                        (*state).tabs.available_capacity() >= self.tabs.selection_len()
                    })
        });
        self.update_drop_preview(gesture.target, target);
        gesture.target = target;
        self.tab_drag = Some(gesture);
        true
    }

    pub(super) unsafe fn finish_tab_pointer(&mut self, point: Point) -> bool {
        let Some(gesture) = self.tab_drag.take() else {
            return false;
        };
        ReleaseCapture();
        if !gesture.dragging {
            if gesture.collapse_on_release {
                self.suspend_active_tab_ui();
                self.tabs.activate_exclusive(gesture.pressed);
                self.restore_active_tab_ui();
            }
            return true;
        }

        let mut screen = point;
        ClientToScreen(self.window, &mut screen);
        let previous = gesture.target;
        let target = self.app.tab_drop_target(screen);
        self.update_drop_preview(previous, None);
        match target {
            Some(target) if target.window == self.window as usize => {
                self.tabs.reorder_selected(target.index);
                InvalidateRect(self.window, null(), 0);
            }
            Some(target) => {
                if let Some(pointer) = self.app.state_pointer(target.window as Hwnd) {
                    let target_state = &mut *pointer;
                    if !self.transfer_selected_tabs(target_state, target.index, None) {
                        self.set_status("The target window cannot accept these tabs");
                    }
                }
            }
            None => self.detach_selected_tabs(screen),
        }
        true
    }

    pub(super) unsafe fn cancel_tab_pointer(&mut self) {
        if let Some(gesture) = self.tab_drag.take() {
            self.update_drop_preview(gesture.target, None);
        }
    }

    pub(super) unsafe fn update_tab_hover(&mut self, point: Point) {
        if self.tab_drag.is_some() {
            return;
        }
        let mut client: Rect = std::mem::zeroed();
        GetClientRect(self.window, &mut client);
        let hovered = match self
            .tab_strip_layout(client.right)
            .hit_test(point.x, point.y)
        {
            Some(TabStripHit::Activate(id)) => Some(id),
            _ => None,
        };
        if self.hovered_tab != hovered {
            self.hovered_tab = hovered;
            InvalidateRect(self.window, null(), 0);
        }
    }

    pub(super) unsafe fn move_selected_tabs(&mut self, forward: bool) {
        if self.tabs.move_selected(forward) {
            InvalidateRect(self.window, null(), 0);
        }
    }

    unsafe fn update_drop_preview(
        &mut self,
        previous: Option<TabDropTarget>,
        next: Option<TabDropTarget>,
    ) {
        if previous == next {
            return;
        }
        self.set_drop_preview(previous, None);
        self.set_drop_preview(next, next.map(|target| target.index));
    }

    unsafe fn set_drop_preview(&mut self, target: Option<TabDropTarget>, index: Option<usize>) {
        let Some(target) = target else {
            return;
        };
        if target.window == self.window as usize {
            self.tab_drop_index = index;
            InvalidateRect(self.window, null(), 0);
        } else if let Some(pointer) = self.app.state_pointer(target.window as Hwnd) {
            (*pointer).tab_drop_index = index;
            InvalidateRect(target.window as Hwnd, null(), 0);
        }
    }

    unsafe fn detach_selected_tabs(&mut self, screen: Point) {
        let mut source_bounds: Rect = std::mem::zeroed();
        GetWindowRect(self.window, &mut source_bounds);
        let state = BrowserState::detached_placeholder(Rc::clone(&self.app));
        let placeholder = state.tabs.active_id();
        let placement = BrowserWindowPlacement::detached(source_bounds, screen, self.dpi);
        let window = match create_browser_window(state, placement) {
            Ok(window) => window,
            Err(error) => {
                self.set_status(&error);
                return;
            }
        };
        let Some(pointer) = self.app.state_pointer(window) else {
            DestroyWindow(window);
            self.set_status("Detached window did not retain its browser state");
            return;
        };
        let target = &mut *pointer;
        if !self.transfer_selected_tabs(target, 0, Some(placeholder)) {
            DestroyWindow(window);
            self.set_status("Could not detach the selected tabs");
            return;
        }
        target.complete_detached_startup();
        ShowWindow(window, SW_SHOW);
        UpdateWindow(window);
        SetForegroundWindow(window);
    }

    unsafe fn transfer_selected_tabs(
        &mut self,
        target: &mut BrowserState,
        insertion: usize,
        placeholder: Option<TabId>,
    ) -> bool {
        debug_assert!(!std::ptr::eq(self, target));
        let moving = self.tabs.selection_len();
        let capacity = target.tabs.available_capacity() + usize::from(placeholder.is_some());
        if moving == 0 || moving > capacity {
            return false;
        }
        let close_source = moving == self.tabs.len();
        self.suspend_active_tab_ui();
        target.suspend_active_tab_ui();
        let fallback = BrowserTab::new(TabId::allocate());
        let mut batch = self.tabs.extract_selected(fallback);
        let moved_ids = batch.tabs.iter().map(|tab| tab.id).collect::<Vec<_>>();
        for tab in &mut batch.tabs {
            tab.page_controls.clear();
            tab.layout_dirty = true;
            if tab.render_dpi != target.dpi {
                tab.render_dpi = target.dpi;
                tab.dynamic_fonts.clear();
            }
        }
        target.tabs.insert_batch(insertion, batch);
        for id in moved_ids {
            self.app.tab_router.bind(id, target.window);
        }
        if let Some(placeholder) = placeholder
            && let Some(tab) = target.tabs.remove(placeholder)
        {
            target.app.tab_router.unbind(placeholder);
            target.remove_renderer_tab(placeholder);
            drop(tab);
        }
        target.ensure_renderer_monitoring();
        target.restore_active_tab_ui();
        if close_source {
            PostMessageW(self.window, WM_CLOSE, 0, 0);
        } else {
            self.ensure_renderer_monitoring();
            self.restore_active_tab_ui();
        }
        SetForegroundWindow(target.window);
        true
    }
}
