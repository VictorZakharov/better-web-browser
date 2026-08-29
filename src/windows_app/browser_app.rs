//! Process-wide browser services and UI-thread window ownership.

use super::renderer_lifecycle::{RendererTaskRegistry, SharedRendererRegistry};
use super::tab_state::ClosedTab;
use super::tabs::{RecentlyClosedTabs, TabId};
use super::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::{Arc, Mutex};

#[derive(Clone, Default)]
pub(super) struct TabMessageRouter {
    // Background work addresses a stable tab identity instead of a window.
    // Rebinding here lets in-flight completions follow a detached/redocked tab.
    destinations: Arc<Mutex<HashMap<TabId, usize>>>,
}

impl TabMessageRouter {
    pub(super) fn bind(&self, id: TabId, window: Hwnd) {
        self.destinations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .insert(id, window as usize);
    }

    pub(super) fn unbind(&self, id: TabId) {
        self.destinations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .remove(&id);
    }

    pub(super) fn destination(&self, id: TabId) -> Option<usize> {
        self.destinations
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .get(&id)
            .copied()
    }
}

pub(super) struct BrowserApplication {
    pub(super) instance: Hinstance,
    pub(super) metrics: Arc<BrowserMetrics>,
    pub(super) http_client: Arc<winhttp::HttpClient>,
    pub(super) local_storage: Arc<better_web_browser::storage::LocalStorage>,
    pub(super) renderer_registry: SharedRendererRegistry,
    pub(super) tab_router: TabMessageRouter,
    pub(super) prefers_dark_color_scheme: Cell<bool>,
    windows: RefCell<Vec<Hwnd>>,
    recently_closed_tabs: RefCell<RecentlyClosedTabs<ClosedTab>>,
}

impl BrowserApplication {
    pub(super) fn new(
        instance: Hinstance,
        metrics: Arc<BrowserMetrics>,
    ) -> Result<Rc<Self>, String> {
        let profile = super::profile::directory()?;
        Ok(Rc::new(Self {
            instance,
            metrics,
            http_client: Arc::new(winhttp::HttpClient::with_profile(&profile)?),
            local_storage: Arc::new(
                better_web_browser::storage::LocalStorage::open(profile.join("local-storage.json"))
                    .map_err(|error| error.to_string())?,
            ),
            renderer_registry: Arc::new(Mutex::new(RendererTaskRegistry::default())),
            tab_router: TabMessageRouter::default(),
            prefers_dark_color_scheme: Cell::new(super::color_scheme::prefers_dark_color_scheme()),
            windows: RefCell::new(Vec::new()),
            recently_closed_tabs: RefCell::new(RecentlyClosedTabs::new()),
        }))
    }

    pub(super) fn register_window(&self, window: Hwnd) {
        let mut windows = self.windows.borrow_mut();
        if !windows.contains(&window) {
            windows.push(window);
        }
    }

    pub(super) fn unregister_window(&self, window: Hwnd) {
        self.windows
            .borrow_mut()
            .retain(|candidate| *candidate != window);
    }

    pub(super) fn window_count(&self) -> usize {
        self.windows.borrow().len()
    }

    pub(super) fn window_handles(&self) -> Vec<Hwnd> {
        self.windows.borrow().clone()
    }

    pub(super) fn remember_closed_tab(&self, tab: ClosedTab) {
        self.recently_closed_tabs.borrow_mut().push(tab);
    }

    pub(super) fn pop_closed_tab(&self) -> Option<ClosedTab> {
        self.recently_closed_tabs.borrow_mut().pop()
    }

    pub(super) fn recent_closed_tabs(&self) -> Vec<ClosedTab> {
        self.recently_closed_tabs
            .borrow()
            .iter_newest()
            .cloned()
            .collect()
    }

    pub(super) fn take_closed_tab(&self, id: u64) -> Option<ClosedTab> {
        self.recently_closed_tabs
            .borrow_mut()
            .remove_where(|tab| tab.id == id)
    }

    pub(super) unsafe fn state_pointer(&self, window: Hwnd) -> Option<*mut BrowserState> {
        if !self.windows.borrow().contains(&window) {
            return None;
        }
        let pointer = GetWindowLongPtrW(window, GWLP_USERDATA) as *mut BrowserState;
        (!pointer.is_null()).then_some(pointer)
    }

    pub(super) unsafe fn browser_for_message(
        &self,
        message_window: Hwnd,
    ) -> Option<(Hwnd, *mut BrowserState)> {
        let parent = GetParent(message_window);
        for window in self.window_handles() {
            let Some(state) = self.state_pointer(window) else {
                continue;
            };
            let popup = (*state).tab_search_window;
            if message_window == window
                || parent == window
                || (!popup.is_null() && (message_window == popup || parent == popup))
            {
                return Some((window, state));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_message_routes_follow_a_tab_between_windows_and_clear_on_close() {
        let router = TabMessageRouter::default();
        let id = TabId::allocate();
        router.bind(id, 0x101_usize as Hwnd);
        assert_eq!(router.destination(id), Some(0x101));

        router.bind(id, 0x202_usize as Hwnd);
        assert_eq!(router.destination(id), Some(0x202));

        router.unbind(id);
        assert_eq!(router.destination(id), None);
    }
}
