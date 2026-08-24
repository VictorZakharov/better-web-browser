//! Browser-owned AccessKit adapter and active-document accessibility boundary.

mod actions;
mod document;
mod tree;

pub(super) use document::{AccessibilityDocument, AppliedAccessibilityUpdate};

use self::actions::{AccessibilityActionForwarder, AccessibilityActionQueue};
use super::*;
use accesskit::{ActivationHandler, TreeUpdate};
use accesskit_windows::Adapter;
use std::cell::RefCell;
use std::sync::Arc;

pub(super) struct AccessibilityState {
    adapter: Option<RefCell<Adapter>>,
    actions: Arc<AccessibilityActionQueue>,
}

impl Default for AccessibilityState {
    fn default() -> Self {
        Self {
            adapter: None,
            actions: AccessibilityActionQueue::new(null_mut()),
        }
    }
}

struct InitialTree<'a>(&'a BrowserState);

impl ActivationHandler for InitialTree<'_> {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        Some(unsafe { tree::full_update(self.0, true) })
    }
}

impl BrowserState {
    pub(super) unsafe fn initialize_accessibility(&mut self) {
        let actions = AccessibilityActionQueue::new(self.window);
        let adapter = Adapter::new(
            accesskit_windows::HWND(self.window),
            false,
            AccessibilityActionForwarder(Arc::clone(&actions)),
        );
        self.accessibility.actions = actions;
        self.accessibility.adapter = Some(RefCell::new(adapter));
    }

    pub(super) unsafe fn handle_accessibility_getobject(
        &self,
        wparam: Wparam,
        lparam: Lparam,
    ) -> Option<Lresult> {
        let mut initial = InitialTree(self);
        let adapter = self.accessibility.adapter.as_ref()?;
        let mut adapter = adapter.borrow_mut();
        let result = adapter.handle_wm_getobject(
            accesskit_windows::WPARAM(wparam),
            accesskit_windows::LPARAM(lparam),
            &mut initial,
        );
        drop(adapter);
        result.map(|result| {
            let result: accesskit_windows::LRESULT = result.into();
            result.0
        })
    }

    pub(super) fn update_accessibility_window_focus(&self, focused: bool) {
        let Some(adapter) = self.accessibility.adapter.as_ref() else {
            return;
        };
        let events = adapter.borrow_mut().update_window_focus_state(focused);
        if let Some(events) = events {
            events.raise();
        }
    }

    pub(super) unsafe fn refresh_accessibility_chrome(&self) {
        self.submit_accessibility_update(|| tree::chrome_update(self));
    }

    pub(super) unsafe fn refresh_accessibility_full(&self) {
        self.submit_accessibility_update(|| tree::full_update(self, false));
    }

    pub(super) unsafe fn refresh_accessibility_document(
        &self,
        update: &AppliedAccessibilityUpdate,
    ) {
        self.submit_accessibility_update(|| {
            tree::document_update(self, &update.changed, update.full)
        });
    }

    pub(super) unsafe fn refresh_accessibility_document_bounds(&self) {
        self.submit_accessibility_update(|| tree::document_bounds_update(self));
    }

    fn submit_accessibility_update(&self, update: impl FnOnce() -> TreeUpdate) {
        let Some(adapter) = self.accessibility.adapter.as_ref() else {
            return;
        };
        let events = adapter.borrow_mut().update_if_active(update);
        if let Some(events) = events {
            events.raise();
        }
    }
}
