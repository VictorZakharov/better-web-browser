//! Browser-to-renderer native input translation and document-scoped sequencing.

#[cfg(test)]
mod activation_tests;
mod keyboard;
mod native_text;
mod pointer;
mod queue;
mod selection;
mod wheel;

pub(super) use pointer::current_buttons;

use super::renderer_input_queue::QueueResult;
use super::tab_state::TabFocus;
use super::tabs::TabId;
use super::*;
use better_web_browser::engine::dom::NodeId;
use better_web_browser::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, FocusInput, InputModifiers, KeyPhase,
    KeyboardInput, LifecycleInput, NativeTextInput, PointerButton, PointerInput, PointerPhase,
    PresentationAcknowledgement, ScrollInput, TextInput,
};
use keyboard::key_and_code;

const RENDERER_INPUT_POLL_BUDGET: u8 = 16;

impl BrowserState {
    pub(in crate::windows_app) fn next_renderer_input(
        &mut self,
    ) -> Option<(better_web_browser::renderer_protocol::DocumentId, u64)> {
        let document = self.navigation.active_document()?;
        self.renderer_input_sequence = self.renderer_input_sequence.checked_add(1)?;
        Some((document, self.renderer_input_sequence))
    }

    pub(super) unsafe fn route_content_pointer(
        &mut self,
        x: i32,
        y: i32,
        phase: PointerPhase,
        button: PointerButton,
        wparam: Wparam,
    ) -> bool {
        if let Some(owner) = self.locked_pointer_owner() {
            return self.route_locked_pointer(owner, x, y, phase, button, wparam);
        }
        if self.surface != Surface::Page {
            return false;
        }
        let toolbar = self.toolbar_height();
        if x < 0 || y < toolbar || y > toolbar + self.viewport_height() {
            self.reset_pointer_cursor();
            return false;
        }
        let scale = self.page_scale().max(f32::EPSILON);
        let document_x = x as f32 / scale;
        let document_y = (y - toolbar + self.scroll_y) as f32 / scale;
        let Some((document, sequence)) = self.next_renderer_input() else {
            return false;
        };
        let accepted = self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase,
            button,
            buttons: pointer::buttons_from_wparam(wparam),
            x: document_x,
            y: document_y,
            modifiers: pointer_modifiers(wparam),
            target: None,
        }));
        // HTML activation starts on a trusted mouse pointerdown. Do not mint a
        // second activation on its matching pointerup after an API consumes it.
        if accepted
            && pointer_starts_activation(
                phase,
                button,
                document,
                &mut self.primary_pointer_down_activation,
            )
        {
            self.transient_activation = Some((document, Instant::now()));
        }
        self.incidents
            .record_pointer(accepted, phase, button, sequence);
        if accepted && phase == PointerPhase::Move {
            self.pointer_cursor_request = Some(sequence);
        }
        accepted
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
        let accepted = self.submit_renderer_input(DocumentInput::Pointer(PointerInput {
            document,
            sequence,
            phase: PointerPhase::Activate,
            button: PointerButton::Primary,
            buttons: 0,
            x: (spec.rect.x + spec.rect.width / 2.0).max(0.0),
            y: (spec.rect.y + spec.rect.height / 2.0).max(0.0),
            modifiers: current_modifiers(),
            target: Some(target),
        }));
        if accepted {
            self.transient_activation = Some((document, Instant::now()));
        }
    }

    pub(super) unsafe fn route_page_control_text(&mut self, index: usize) {
        // Until the new document's first presentation, the old page's HWNDs can still be
        // visible. Never address their node IDs to the replacement renderer.
        if self.renderer_revision == 0 || self.suppress_page_control_focus {
            return;
        }
        let Some(control) = self.page_controls.get(index) else {
            return;
        };
        let window = control.window;
        let spec = control.spec.clone();
        let (value, selection_start, selection_end) = if spec.kind == ControlKind::Select {
            let selected = SendMessageW(window, CB_GETCURSEL, 0, 0);
            let value = (selected >= 0)
                .then_some(selected as usize)
                .and_then(|selected| spec.options.get(selected))
                .map(|option| option.value.clone())
                .unwrap_or_default();
            let end = value.encode_utf16().count().min(u32::MAX as usize) as u32;
            (value, end, end)
        } else {
            page_controls::selection::edit_text_and_selection(window, spec.kind)
        };
        let Some(target) = wire_node(spec.node_id) else {
            return;
        };
        let Some((document, sequence)) = self.next_renderer_input() else {
            return;
        };
        let text = TextInput {
            document,
            sequence,
            target,
            value: value.clone(),
            selection_start,
            selection_end,
        };
        let input = if matches!(
            spec.kind,
            ControlKind::Text | ControlKind::Search | ControlKind::Password
        ) {
            DocumentInput::NativeText(NativeTextInput {
                text,
                generation: self.native_text_generation,
                intent: super::window_dispatch::current_page_edit_intent(),
                pre_selection: super::window_dispatch::current_page_edit_selection(window),
            })
        } else {
            DocumentInput::Text(text)
        };
        let accepted = self.submit_renderer_input(input);
        if accepted
            && spec.kind != ControlKind::Select
            && let Some(control) = self.page_controls.get_mut(index)
        {
            control.last_text = value;
            control.last_selection = (selection_start, selection_end);
            control.last_native_selection = page_controls::selection::read_edit_selection(window);
            control.last_direction =
                better_web_browser::renderer_protocol::TextSelectionDirection::None;
            control.last_native_input_sequence = sequence;
        }
    }

    pub(super) unsafe fn route_page_control_focus(&mut self, id: usize, focused: bool) {
        if self.renderer_revision == 0 {
            return;
        }
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
        let activates = matches!(message, WM_KEYDOWN | WM_SYSKEYDOWN) && virtual_key != VK_ESCAPE;
        let accepted = self.submit_renderer_input(DocumentInput::Keyboard(KeyboardInput {
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
        if accepted && activates {
            self.transient_activation = Some((document, Instant::now()));
        }
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

pub(super) fn wire_node(node: NodeId) -> Option<DocumentNodeId> {
    DocumentNodeId::new(node.to_wire()).ok()
}

fn pointer_starts_activation(
    phase: PointerPhase,
    button: PointerButton,
    document: better_web_browser::renderer_protocol::DocumentId,
    down_document: &mut Option<better_web_browser::renderer_protocol::DocumentId>,
) -> bool {
    if button != PointerButton::Primary {
        return false;
    }
    match phase {
        PointerPhase::Down => {
            *down_document = Some(document);
            true
        }
        PointerPhase::Up => down_document.take() != Some(document),
        PointerPhase::Activate => true,
        _ => false,
    }
}

pub(super) unsafe fn pointer_modifiers(wparam: Wparam) -> InputModifiers {
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
