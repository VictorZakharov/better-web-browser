//! Renderer-owned hit testing, DOM event dispatch, default actions, and input sequencing.

mod default_actions;
mod history;
mod hit_testing;
mod native_text;
mod pointer;
mod rendering;
mod scrolling;
mod selection;
mod state_render;
mod viewport;
use state_render::request_state_render;

use super::*;
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::{ControlKind, ControlSpec, DisplayItem, UserInputEvent, UserInputModifiers};
use crate::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, InputModifiers, KeyPhase, NativeTextInput,
    NativeTextRejection, NavigationDisposition, PointerButton, PointerCursor, PointerCursorResult,
    PointerInput, PointerPhase, PresentationAcknowledgement,
};

pub(in crate::renderer_process::child) struct InteractionResult {
    pub(in crate::renderer_process::child) presentation: Option<AdvanceResult>,
    pub(in crate::renderer_process::child) navigation: Option<(String, NavigationDisposition)>,
    pub(in crate::renderer_process::child) cursor: Option<PointerCursorResult>,
}
struct PointerInteraction {
    outcome: ScriptOutcome,
    navigation: Option<(String, NavigationDisposition)>,
    cursor: Option<PointerCursorResult>,
}

impl DocumentRuntime {
    pub(in crate::renderer_process::child) fn interact(
        &mut self,
        input: DocumentInput,
        connection: &mut ChildConnection,
    ) -> Result<InteractionResult, String> {
        input.validate().map_err(|error| error.to_string())?;
        if input.document() != self.id || input.sequence() <= self.last_input_sequence {
            return Ok(InteractionResult {
                presentation: None,
                navigation: None,
                cursor: None,
            });
        }
        self.last_input_sequence = input.sequence();
        self.media_activation.observe(&input);
        if let Some(runtime) = self.script_runtime.as_mut() {
            runtime.set_audio_activation(self.media_activation.allows(1_000));
        }
        let force_accessibility_update = matches!(
            &input,
            DocumentInput::Text(_)
                | DocumentInput::NativeText(_)
                | DocumentInput::Selection(_)
                | DocumentInput::Focus(_)
        ) || (matches!(&input, DocumentInput::Scroll(_))
            && !self.layout.sticky_offsets.is_empty());
        let mut cursor = None;
        let mut history_traversal_ack = None;
        let mut native_text_rejection = None;
        let mut wheel_acknowledgement = None;
        let (mut outcome, navigation) = match input {
            DocumentInput::Wheel(input) => {
                let sequence = input.sequence;
                let started = Instant::now();
                let (outcome, decision) = self.wheel_input(input)?;
                wheel_acknowledgement = Some(crate::renderer_protocol::WheelAcknowledgement {
                    sequence,
                    decision,
                    viewport_delta_y: outcome.viewport_wheel_delta_y,
                    dispatch_micros: micros(started.elapsed()),
                });
                (outcome, None)
            }
            DocumentInput::Pointer(input) => {
                let interaction = self.pointer_input(input)?;
                cursor = interaction.cursor;
                (interaction.outcome, interaction.navigation)
            }
            DocumentInput::Keyboard(input) => {
                let scroll_key = (input.phase == KeyPhase::Down).then(|| input.key.clone());
                let key_code = key_code(&input.key);
                let activates_form = input.phase == KeyPhase::Down && input.key == "Enter";
                // Keyboard events target the focused area, which can be inside a
                // shadow tree even when UI accessibility exposes its host. The
                // input packet's target is only a fallback when no area is focused.
                let target = self
                    .focused_node
                    .and_then(|id| self.page.dom.find_node(id))
                    .or_else(|| input.target.and_then(|target| self.resolve_target(target)));
                let target_id = target.as_ref().map(|node| node.id());
                let result = self.dispatch_user_input(UserInputEvent::Keyboard {
                    target,
                    phase: match input.phase {
                        KeyPhase::Down => "down",
                        KeyPhase::Up => "up",
                    },
                    key: input.key,
                    code: input.code,
                    key_code,
                    repeat: input.repeat,
                    modifiers: input.modifiers.into(),
                })?;
                let mut outcome = result.outcome;
                if result.default_allowed
                    && let Some(key) = scroll_key
                    && let Some(scrolled) = self.scroll_key(&key)?
                {
                    merge_outcome(&mut outcome, scrolled, self.page.dom.document.id());
                }
                let navigation = if activates_form && result.default_allowed {
                    self.keyboard_default_action(target_id, &mut outcome)?
                } else {
                    None
                };
                (outcome, navigation)
            }
            DocumentInput::Text(input) => {
                let Some(target) = self.resolve_target(input.target) else {
                    return Ok(InteractionResult {
                        presentation: None,
                        navigation: None,
                        cursor: None,
                    });
                };
                self.accessibility_selection =
                    Some((target.id(), input.selection_start, input.selection_end));
                let mut result = self.dispatch_user_input(UserInputEvent::Text {
                    target: target.clone(),
                    value: input.value.clone(),
                    selection_start: input.selection_start,
                    selection_end: input.selection_end,
                })?;
                if self.script_runtime.is_none() {
                    // Scriptless documents still edit authoritative control
                    // state; layout, paint, and submission read it from there.
                    self.accessibility_values
                        .insert(target.id(), input.value.clone());
                    let changed = match target.tag_name() {
                        Some("input") => target.user_edit_input(&input.value),
                        Some("textarea") => target.set_textarea_raw(&input.value, true),
                        Some("select") => target.user_pick_option(&input.value),
                        _ => false,
                    };
                    if changed {
                        request_state_render(&target, &mut result.outcome);
                    }
                }
                (result.outcome, None)
            }
            DocumentInput::NativeText(input) => {
                let Some((outcome, rejection)) = self.native_text_input(input)? else {
                    return Ok(InteractionResult {
                        presentation: None,
                        navigation: None,
                        cursor: None,
                    });
                };
                native_text_rejection = rejection;
                (outcome, None)
            }
            DocumentInput::Selection(input) => (self.selection_input(input)?, None),
            DocumentInput::Focus(input) => {
                if !input.focused {
                    self.pointer_down = [None; 3];
                    self.scroll_drag = None;
                }
                let previous = self.focused_node.and_then(|id| self.page.dom.find_node(id));
                let target = input.target.and_then(|target| self.resolve_target(target));
                if self.script_runtime.is_none() {
                    crate::engine::dom::Node::set_focus_target(
                        previous.as_ref(),
                        input.focused.then_some(target.as_ref()).flatten(),
                    );
                }
                self.focused_node = input
                    .focused
                    .then(|| target.as_ref().map(|node| node.id()))
                    .flatten();
                let result = self.dispatch_user_input(UserInputEvent::Focus {
                    target,
                    focused: input.focused,
                })?;
                (result.outcome, None)
            }
            DocumentInput::Scroll(input) => {
                self.page.dom.document.scroll_offset.set((input.x, input.y));
                if !self.layout.sticky_offsets.is_empty() {
                    self.layout.update_sticky_positions(
                        &self.page,
                        self.viewport.width,
                        self.viewport.height,
                        self.viewport.style_width,
                    );
                    if let Some(runtime) = self.script_runtime.as_mut() {
                        runtime.set_sticky_offsets(&self.layout.sticky_offsets);
                    }
                }
                let result = self.dispatch_user_input(UserInputEvent::Scroll {
                    x: input.x,
                    y: input.y,
                })?;
                (result.outcome, None)
            }
            DocumentInput::Lifecycle(input) => {
                if input.state != DocumentLifecycle::Active {
                    self.pointer_down = [None; 3];
                    self.scroll_drag = None;
                }
                let previous = lifecycle_name(self.lifecycle);
                self.lifecycle = input.state;
                let result = self.dispatch_user_input(UserInputEvent::Lifecycle {
                    state: lifecycle_name(input.state),
                    previous,
                })?;
                (result.outcome, None)
            }
            DocumentInput::History(input) => {
                history_traversal_ack = Some(input.sequence);
                (self.apply_history_input(input), None)
            }
        };
        self.admit_user_input_outcome(&mut outcome, connection)?;
        if let Some(wheel) = wheel_acknowledgement.as_mut() {
            // Admission can run callbacks with a later absolute scroll. Retain
            // only the default-action contribution that survives that barrier.
            wheel.viewport_delta_y = outcome.viewport_wheel_delta_y;
        }
        let mut presentation = self.presentation_after_user_input(
            outcome,
            force_accessibility_update,
            wheel_acknowledgement,
            connection,
        )?;
        if let Some(sequence) = history_traversal_ack {
            match presentation.as_mut() {
                Some(AdvanceResult::Presentation(presentation)) => {
                    presentation.runtime.history_traversal_ack = Some(sequence);
                }
                Some(AdvanceResult::Runtime(update)) => {
                    update.runtime.history_traversal_ack = Some(sequence);
                }
                _ => return Err("history traversal produced no renderer report".into()),
            }
        }
        if let Some(rejection) = native_text_rejection {
            native_text::attach_rejection(&mut presentation, rejection)?;
        }
        Ok(InteractionResult {
            presentation,
            navigation,
            cursor,
        })
    }

    pub(in crate::renderer_process::child) fn acknowledge_presentation(
        &mut self,
        acknowledgement: PresentationAcknowledgement,
    ) -> Result<(), String> {
        acknowledgement
            .validate()
            .map_err(|error| error.to_string())?;
        if acknowledgement.document != self.id
            || acknowledgement.revision <= self.last_acknowledged_revision
        {
            return Ok(());
        }
        if acknowledgement.revision > self.revision {
            return Err("browser acknowledged an unsent presentation revision".into());
        }
        self.last_acknowledged_revision = acknowledgement.revision;
        Ok(())
    }

    pub(super) fn admit_user_input_outcome(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        // Media acknowledgements can run callbacks that produce more side effects.
        self.collect_document_stream_changes();
        // Collect those after the bounded media-action drain, not before it.
        self.apply_media_actions(outcome, connection)?;
        self.apply_graph_audio_actions(outcome, connection)?;
        self.apply_font_actions(outcome);
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.pending_websockets
            .append(&mut outcome.websocket_actions);
        self.pending_databases.append(&mut outcome.database_actions);
        self.pending_speech_requests
            .append(&mut outcome.speech_actions);
        self.pending_notification_requests
            .append(&mut outcome.notification_actions);
        self.pending_protocol_handler_requests
            .append(&mut outcome.protocol_handler_actions);
        self.pending_permission_requests
            .append(&mut outcome.permission_actions);
        self.pending_geolocation_requests
            .append(&mut outcome.geolocation_actions);
        self.pending_media_device_requests
            .append(&mut outcome.media_device_actions);
        self.pending_sensor_requests
            .append(&mut outcome.sensor_actions);
        self.pending_clipboard_requests
            .append(&mut outcome.clipboard_actions);
        self.pending_worker_actions
            .append(&mut outcome.worker_actions);
        self.start_file_picker_requests(outcome, connection)?;
        connection.send_network_state_updates(self.id, outcome)?;
        self.start_pending_survivable_fetches(connection)?;
        self.start_pending_speech_requests(connection)?;
        self.start_pending_notification_requests(connection)?;
        self.start_pending_protocol_handler_requests(connection)?;
        self.start_pending_permission_requests(connection)?;
        self.start_pending_geolocation_requests(connection)?;
        self.start_pending_media_device_requests(connection)?;
        self.start_pending_sensor_requests(connection)?;
        self.start_pending_clipboard_requests(connection)?;
        connection.send_state_mutations(self.id, self.last_input_sequence, outcome)
    }
}

struct HitTarget {
    node: NodeRef,
    link: Option<String>,
    control: Option<ControlSpec>,
}

fn cursor_for_target(target: Option<&HitTarget>) -> PointerCursor {
    cursor_for_link(target.is_some_and(|target| target.link.is_some()))
}

fn cursor_for_link(actionable_link: bool) -> PointerCursor {
    if actionable_link {
        PointerCursor::Pointer
    } else {
        PointerCursor::Default
    }
}

impl From<InputModifiers> for UserInputModifiers {
    fn from(modifiers: InputModifiers) -> Self {
        Self {
            alt: modifiers.alt,
            control: modifiers.control,
            shift: modifiers.shift,
            meta: modifiers.meta,
        }
    }
}

fn contains(rect: crate::engine::RectF, x: f32, y: f32) -> bool {
    x >= rect.x && x <= rect.right() && y >= rect.y && y <= rect.bottom()
}

fn lifecycle_name(state: DocumentLifecycle) -> &'static str {
    match state {
        DocumentLifecycle::Active => "active",
        DocumentLifecycle::Hidden => "hidden",
        DocumentLifecycle::Frozen => "frozen",
    }
}

fn key_code(key: &str) -> u32 {
    match key {
        "Backspace" => 8,
        "Tab" => 9,
        "Enter" => 13,
        "Escape" => 27,
        " " => 32,
        "ArrowLeft" => 37,
        "ArrowUp" => 38,
        "ArrowRight" => 39,
        "ArrowDown" => 40,
        _ => key
            .chars()
            .next()
            .filter(|_| key.chars().count() == 1)
            .map_or(0, |c| c.to_ascii_uppercase() as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hit_target_cursor_distinguishes_links_from_ordinary_content() {
        assert_eq!(cursor_for_link(true), PointerCursor::Pointer);
        assert_eq!(cursor_for_link(false), PointerCursor::Default);
    }

    #[test]
    fn keyboard_compatibility_codes_match_native_windows_input() {
        assert_eq!(key_code("k"), 75);
        assert_eq!(key_code("ArrowRight"), 39);
        assert_eq!(key_code("Escape"), 27);
    }
}
