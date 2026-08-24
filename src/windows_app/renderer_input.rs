//! Browser-to-renderer native input translation and document-scoped sequencing.

use super::renderer_input_queue::QueueResult;
use super::tab_state::TabFocus;
use super::tabs::TabId;
use super::*;
use better_web_browser::engine::dom::NodeId;
use better_web_browser::limits::MAX_QUEUED_BROWSER_COMMANDS;
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, FocusInput, InputModifiers, KeyPhase,
    KeyboardInput, LifecycleInput, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement, ScrollInput, TextInput,
};

const RENDERER_INPUT_POLL_BUDGET: u8 = 16;

impl BrowserState {
    pub(in crate::windows_app) fn next_renderer_input(
        &mut self,
    ) -> Option<(better_web_browser::renderer_protocol::DocumentId, u64)> {
        let document = self.renderer_document?;
        self.renderer_input_sequence = self.renderer_input_sequence.checked_add(1)?;
        Some((document, self.renderer_input_sequence))
    }

    pub(in crate::windows_app) fn submit_renderer_input(&mut self, input: DocumentInput) -> bool {
        let result = if self.pending_renderer_inputs.is_empty() {
            self.renderer_session
                .as_ref()
                .ok_or_else(|| "renderer session is unavailable".to_string())
                .and_then(|session| session.try_send_input_retained(input))
        } else {
            input
                .validate()
                .map_err(|error| error.to_string())
                .map(|()| Some(input))
        };
        match result {
            Ok(None) => {
                self.note_renderer_input_activity();
                true
            }
            Ok(Some(input)) => match self.pending_renderer_inputs.enqueue(input) {
                QueueResult::Queued | QueueResult::Coalesced => {
                    self.note_renderer_input_activity();
                    true
                }
                QueueResult::Full => {
                    self.note_renderer_input_activity();
                    unsafe {
                        self.set_status(
                            "Renderer is busy; this input was not accepted. Try again.",
                        );
                    }
                    false
                }
            },
            Err(error) => {
                unsafe {
                    self.contain_page_engine_failure(
                        self.id,
                        format!("could not deliver document input: {error}"),
                    );
                }
                false
            }
        }
    }

    fn note_renderer_input_activity(&mut self) {
        self.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
        unsafe {
            self.ensure_renderer_monitoring();
        }
    }

    pub(super) unsafe fn route_content_pointer(
        &mut self,
        x: i32,
        y: i32,
        phase: PointerPhase,
        button: PointerButton,
        wparam: Wparam,
    ) -> bool {
        if self.surface != Surface::Page {
            return false;
        }
        let toolbar = self.toolbar_height();
        if x < 0 || y < toolbar || y > toolbar + self.viewport_height() {
            return false;
        }
        let scale = self.page_scale().max(f32::EPSILON);
        let document_x = x as f32 / scale;
        let document_y = (y - toolbar + self.scroll_y) as f32 / scale;
        let Some((document, sequence)) = self.next_renderer_input() else {
            return false;
        };
        self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase,
            button,
            x: document_x,
            y: document_y,
            modifiers: pointer_modifiers(wparam),
            target: None,
        }))
    }

    pub(super) unsafe fn route_page_control_activation(&mut self, index: usize) {
        let Some(spec) = self
            .page_controls
            .get(index)
            .map(|control| control.spec.clone())
        else {
            return;
        };
        let Some(target) = wire_node(spec.node_id) else {
            return;
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            x: (spec.rect.x + spec.rect.width / 2.0).max(0.0),
            y: (spec.rect.y + spec.rect.height / 2.0).max(0.0),
            modifiers: current_modifiers(),
            target: Some(target),
        }));
    }

    pub(super) unsafe fn route_page_control_text(&mut self, index: usize) {
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let window = control.window;
        let spec = control.spec.clone();
        let value = if spec.kind == ControlKind::Select {
            let selected = SendMessageW(window, CB_GETCURSEL, 0, 0);
            (selected >= 0)
                .then_some(selected as usize)
                .and_then(|selected| spec.options.get(selected))
                .map(|option| option.value.clone())
                .unwrap_or_default()
        } else {
            window_text(window)
        };
        let (selection_start, selection_end) = if spec.kind == ControlKind::Select {
            let end = value.encode_utf16().count().min(u32::MAX as usize) as u32;
            (end, end)
        } else {
            edit_selection(window)
        };
        let Some(target) = wire_node(spec.node_id) else {
            return;
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Text(TextInput {
            document,
            sequence,
            target,
            value,
            selection_start,
            selection_end,
        }));
    }

    pub(super) unsafe fn route_page_control_focus(&mut self, id: usize, focused: bool) {
        let Some(index) = id.checked_sub(ID_PAGE_CONTROL_BASE) else {
            return;
        };
        let Some(node) = self
            .page_controls
            .get(index)
            .map(|control| control.spec.node_id)
        else {
            return;
        };
        self.focus = if focused {
            TabFocus::PageControl(node)
        } else {
            TabFocus::Content
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Focus(FocusInput {
            document,
            sequence,
            focused,
            target: wire_node(node),
        }));
    }

    pub(super) unsafe fn route_renderer_keyboard(
        &mut self,
        window: Hwnd,
        message: u32,
        virtual_key: usize,
        lparam: Lparam,
    ) {
        let is_key = matches!(message, WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP);
        if !is_key || window == self.controls.address || self.surface != Surface::Page {
            return;
        }
        let target = self
            .page_controls
            .iter()
            .find(|control| control.window == window)
            .and_then(|control| wire_node(control.spec.node_id));
        if window != self.window && target.is_none() {
            return;
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let (key, code) = key_and_code(virtual_key, current_modifiers().shift);
        let _ = self.submit_renderer_input(DocumentInput::Keyboard(KeyboardInput {
            document,
            sequence,
            phase: if matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN) {
                KeyPhase::Down
            } else {
                KeyPhase::Up
            },
            key,
            code,
            repeat: lparam as usize & (1 << 30) != 0,
            modifiers: current_modifiers(),
            target,
        }));
    }

    pub(super) fn route_renderer_scroll(&mut self) {
        if self.surface != Surface::Page {
            return;
        }
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let scale = self.page_scale().max(f32::EPSILON);
        let _ = self.submit_renderer_input(DocumentInput::Scroll(ScrollInput {
            document,
            sequence,
            x: 0.0,
            y: self.scroll_y.max(0) as f32 / scale,
        }));
    }

    pub(super) unsafe fn flush_renderer_inputs_for(&mut self, id: TabId) {
        for _ in 0..MAX_QUEUED_BROWSER_COMMANDS {
            let delivery = {
                let Some(tab) = self.tabs.get_mut(id) else {
                    return;
                };
                let Some(input) = tab.pending_renderer_inputs.pop_front() else {
                    return;
                };
                if tab.renderer_document != Some(input.document()) {
                    continue;
                }
                tab.renderer_session
                    .as_ref()
                    .ok_or_else(|| "renderer session is unavailable".to_string())
                    .and_then(|session| session.try_send_input_retained(input))
            };
            match delivery {
                Ok(None) => {}
                Ok(Some(input)) => {
                    if let Some(tab) = self.tabs.get_mut(id) {
                        tab.pending_renderer_inputs.restore_front(input);
                        tab.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
                    }
                    return;
                }
                Err(error) => {
                    self.contain_page_engine_failure(
                        id,
                        format!("could not deliver document input: {error}"),
                    );
                    return;
                }
            }
        }
        if let Some(tab) = self.tabs.get_mut(id)
            && !tab.pending_renderer_inputs.is_empty()
        {
            tab.renderer_input_poll_budget = RENDERER_INPUT_POLL_BUDGET;
        }
    }

    pub(super) fn route_renderer_lifecycle(&mut self, state: DocumentLifecycle) {
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let _ = self.submit_renderer_input(DocumentInput::Lifecycle(LifecycleInput {
            document,
            sequence,
            state,
        }));
    }

    pub(super) fn acknowledge_renderer_presentation(
        &mut self,
        document: better_web_browser::renderer_protocol::DocumentId,
        revision: u64,
        presented: bool,
        controls_applied: bool,
    ) {
        let result = self.renderer_session.as_ref().map(|session| {
            session.acknowledge_presentation(PresentationAcknowledgement {
                document,
                revision,
                presented,
                controls_applied,
            })
        });
        if let Some(Err(error)) = result {
            unsafe {
                self.contain_page_engine_failure(
                    self.id,
                    format!("could not acknowledge renderer presentation: {error}"),
                );
            }
        }
    }
}

fn wire_node(node: NodeId) -> Option<DocumentNodeId> {
    DocumentNodeId::new(node.to_wire()).ok()
}

unsafe fn edit_selection(window: Hwnd) -> (u32, u32) {
    let mut start = 0_u32;
    let mut end = 0_u32;
    SendMessageW(
        window,
        EM_GETSEL,
        (&mut start as *mut u32) as usize,
        (&mut end as *mut u32) as isize,
    );
    (start, end)
}

unsafe fn pointer_modifiers(wparam: Wparam) -> InputModifiers {
    InputModifiers {
        control: wparam & MK_CONTROL != 0,
        shift: wparam & MK_SHIFT != 0,
        alt: GetKeyState(VK_MENU) < 0,
        meta: false,
    }
}

unsafe fn current_modifiers() -> InputModifiers {
    InputModifiers {
        control: GetKeyState(VK_CONTROL) < 0,
        shift: GetKeyState(VK_SHIFT) < 0,
        alt: GetKeyState(VK_MENU) < 0,
        meta: false,
    }
}

fn key_and_code(key: usize, shift: bool) -> (String, String) {
    match key {
        VK_BACK => ("Backspace".into(), "Backspace".into()),
        VK_TAB => ("Tab".into(), "Tab".into()),
        VK_RETURN => ("Enter".into(), "Enter".into()),
        VK_ESCAPE => ("Escape".into(), "Escape".into()),
        VK_SPACE => (" ".into(), "Space".into()),
        VK_LEFT => ("ArrowLeft".into(), "ArrowLeft".into()),
        VK_UP => ("ArrowUp".into(), "ArrowUp".into()),
        VK_RIGHT => ("ArrowRight".into(), "ArrowRight".into()),
        VK_DOWN => ("ArrowDown".into(), "ArrowDown".into()),
        VK_DELETE => ("Delete".into(), "Delete".into()),
        value @ 0x30..=0x39 => (
            (value as u8 as char).to_string(),
            format!("Digit{}", value - 0x30),
        ),
        value @ 0x41..=0x5a => {
            let letter = value as u8 as char;
            let key = if shift {
                letter
            } else {
                letter.to_ascii_lowercase()
            };
            (key.to_string(), format!("Key{letter}"))
        }
        _ => (format!("Unidentified-{key:02x}"), "Unidentified".into()),
    }
}
