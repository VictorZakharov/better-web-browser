//! Bounded cross-thread AccessKit action queue and validated browser dispatch.

use super::tree::*;
use crate::windows_app::tab_state::TabFocus;
use crate::windows_app::*;
use accesskit::{Action, ActionData, ActionHandler, ActionRequest, NodeId, TreeId};
use better_web_browser::engine::dom::NodeId as EngineNodeId;
use better_web_browser::limits::{MAX_RENDERER_TEXT_INPUT_BYTES, MAX_URL_BYTES};
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentNodeId, FocusInput, InputModifiers, PointerButton, PointerInput,
    PointerPhase, TextInput,
};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

const MAX_QUEUED_ACCESSIBILITY_ACTIONS: usize = 64;

pub(super) struct AccessibilityActionQueue {
    window: usize,
    pending: Mutex<VecDeque<ActionRequest>>,
}

impl AccessibilityActionQueue {
    pub(super) fn new(window: Hwnd) -> Arc<Self> {
        Arc::new(Self {
            window: window as usize,
            pending: Mutex::new(VecDeque::new()),
        })
    }

    fn submit(&self, request: ActionRequest) {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if pending.len() >= MAX_QUEUED_ACCESSIBILITY_ACTIONS {
            return;
        }
        pending.push_back(request);
        drop(pending);
        unsafe {
            PostMessageW(self.window as Hwnd, WM_APP_ACCESSIBILITY_ACTION, 0, 0);
        }
    }

    pub(super) fn drain(&self) -> Vec<ActionRequest> {
        let mut pending = self
            .pending
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        pending.drain(..).collect()
    }
}

pub(super) struct AccessibilityActionForwarder(pub(super) Arc<AccessibilityActionQueue>);

impl ActionHandler for AccessibilityActionForwarder {
    fn do_action(&mut self, request: ActionRequest) {
        self.0.submit(request);
    }
}

impl BrowserState {
    pub(super) unsafe fn route_accessibility_focus(&mut self, target: DocumentNodeId) {
        let target_node = EngineNodeId::from_wire(target.get());
        if let Some(control) = self
            .page_controls
            .iter()
            .find(|control| Some(control.spec.node_id) == target_node)
        {
            SetFocus(control.window);
        } else {
            SetFocus(self.window);
            self.focus = TabFocus::Content;
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Focus(FocusInput {
            document,
            sequence,
            focused: true,
            target: Some(target),
        }));
    }

    pub(super) fn route_accessibility_invoke(
        &mut self,
        target: DocumentNodeId,
        bounds: better_web_browser::engine::RectF,
    ) {
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: (bounds.x + bounds.width / 2.0).max(0.0),
            y: (bounds.y + bounds.height / 2.0).max(0.0),
            modifiers: InputModifiers::default(),
            target: Some(target),
        }));
    }

    pub(super) fn route_accessibility_value(&mut self, target: DocumentNodeId, value: String) {
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let end = value.encode_utf16().count().min(u32::MAX as usize) as u32;
        let _ = self.submit_renderer_input(DocumentInput::Text(TextInput {
            document,
            sequence,
            target,
            value,
            selection_start: end,
            selection_end: end,
        }));
    }

    pub(in crate::windows_app) unsafe fn dispatch_accessibility_actions(&mut self) {
        let requests = self.accessibility.actions.drain();
        for request in requests {
            self.dispatch_accessibility_action(request);
        }
        self.refresh_accessibility_chrome();
    }

    unsafe fn dispatch_accessibility_action(&mut self, request: ActionRequest) {
        if request.target_tree != TreeId::ROOT {
            return;
        }
        if self.dispatch_chrome_accessibility_action(&request) {
            return;
        }
        let Some(target) = self
            .accessibility_document
            .document_id_for_platform(request.target_node.0)
        else {
            return;
        };
        let Some(node) = self.accessibility_document.node(target).cloned() else {
            return;
        };
        match request.action {
            Action::Focus if node.actions.focus => self.route_accessibility_focus(target),
            Action::Click if node.actions.invoke => {
                self.route_accessibility_invoke(target, node.bounds)
            }
            Action::SetValue if node.actions.set_value => {
                let Some(ActionData::Value(value)) = request.data else {
                    return;
                };
                if value.len() <= MAX_RENDERER_TEXT_INPUT_BYTES {
                    self.route_accessibility_value(target, value.into());
                }
            }
            _ => {}
        }
    }

    unsafe fn dispatch_chrome_accessibility_action(&mut self, request: &ActionRequest) -> bool {
        if let Some(tab) = tab_from_node(request.target_node) {
            if matches!(request.action, Action::Click | Action::Focus) && self.tabs.contains(tab) {
                self.activate_tab(tab);
            }
            return true;
        }
        let control = match request.target_node {
            BACK_ID => Some((self.controls.back, ID_BACK)),
            FORWARD_ID => Some((self.controls.forward, ID_FORWARD)),
            RELOAD_ID => Some((self.controls.reload, ID_RELOAD)),
            GO_ID => Some((self.controls.go, ID_GO)),
            READER_ID => Some((self.controls.reader, ID_READER)),
            TASK_MANAGER_ID => Some((self.controls.task_manager, ID_TASK_MANAGER)),
            _ => None,
        };
        if let Some((window, command)) = control {
            match request.action {
                Action::Focus => {
                    SetFocus(window);
                }
                Action::Click => {
                    SendMessageW(self.window, WM_COMMAND, command, 0);
                }
                _ => {}
            }
            return true;
        }
        match request.target_node {
            ADDRESS_ID => {
                match request.action {
                    Action::Focus => {
                        SetFocus(self.controls.address);
                    }
                    Action::SetValue => {
                        if let Some(ActionData::Value(value)) = request.data.as_ref()
                            && value.len() <= MAX_URL_BYTES
                        {
                            self.omnibox_text = value.to_string();
                            set_window_text(self.controls.address, value);
                        }
                    }
                    _ => {}
                }
                true
            }
            NEW_TAB_ID => {
                if request.action == Action::Click {
                    self.new_tab();
                }
                true
            }
            SEARCH_TABS_ID => {
                if request.action == Action::Click {
                    self.toggle_tab_search();
                }
                true
            }
            WINDOW_ID | TAB_LIST_ID | TOOLBAR_ID | STATUS_ID => true,
            NodeId(_) => false,
        }
    }
}
