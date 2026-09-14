//! Renderer-owned hit testing, DOM event dispatch, default actions, and input sequencing.

mod default_actions;
mod hit_testing;
mod pointer;
mod rendering;
mod scrolling;
mod viewport;

use super::*;
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::{ControlKind, ControlSpec, DisplayItem, UserInputEvent, UserInputModifiers};
use crate::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, InputModifiers, KeyPhase,
    NavigationDisposition, PointerButton, PointerCursor, PointerCursorResult, PointerInput,
    PointerPhase, PresentationAcknowledgement,
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
        let force_accessibility_update =
            matches!(&input, DocumentInput::Text(_) | DocumentInput::Focus(_))
                || (matches!(&input, DocumentInput::Scroll(_))
                    && !self.layout.sticky_offsets.is_empty());
        let mut cursor = None;
        let (mut outcome, navigation) = match input {
            DocumentInput::Wheel(input) => (self.wheel_input(input)?, None),
            DocumentInput::Pointer(input) => {
                let interaction = self.pointer_input(input)?;
                cursor = interaction.cursor;
                (interaction.outcome, interaction.navigation)
            }
            DocumentInput::Keyboard(input) => {
                let scroll_key = (input.phase == KeyPhase::Down).then(|| input.key.clone());
                let key_code = key_code(&input.key);
                let activates_form = input.phase == KeyPhase::Down && input.key == "Enter";
                let target = input
                    .target
                    .and_then(|target| self.resolve_target(target))
                    .or_else(|| self.focused_node.and_then(|id| self.page.dom.find_node(id)));
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
                if self.script_runtime.is_none() {
                    self.accessibility_values
                        .insert(target.id(), input.value.clone());
                }
                let result = self.dispatch_user_input(UserInputEvent::Text {
                    target,
                    value: input.value,
                    selection_start: input.selection_start,
                    selection_end: input.selection_end,
                })?;
                (result.outcome, None)
            }
            DocumentInput::Focus(input) => {
                if !input.focused {
                    self.pointer_down = [None; 3];
                    self.scroll_drag = None;
                }
                let target = input.target.and_then(|target| self.resolve_target(target));
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
        };
        self.admit_user_input_outcome(&mut outcome, connection)?;
        let presentation =
            self.presentation_after_user_input(outcome, force_accessibility_update, connection)?;
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
        // Collect those after the bounded media-action drain, not before it.
        self.apply_media_actions(outcome, connection)?;
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.pending_worker_actions
            .append(&mut outcome.worker_actions);
        connection.send_state_mutations(self.id, outcome)
    }

    pub(super) fn presentation_after_user_input(
        &mut self,
        mut outcome: ScriptOutcome,
        force_accessibility_update: bool,
        connection: &mut ChildConnection,
    ) -> Result<Option<AdvanceResult>, String> {
        let needs_present = force_accessibility_update
            || outcome.render_requested
            || outcome.executed > 0
            || !outcome.errors.is_empty()
            || !outcome.console.is_empty()
            || !outcome.diagnostics.is_empty()
            || outcome.navigation_url.is_some()
            || outcome.viewport_scroll_y.is_some()
            || outcome.viewport_wheel_delta_y != 0.0
            || !outcome.history_actions.is_empty()
            || !outcome.cookie_updates.is_empty();
        if !needs_present {
            return Ok(None);
        }
        let style = if outcome.render_requested {
            self.refresh_input_styles(&mut outcome)
        } else {
            StyleRefreshStats::default()
        };
        self.start_presentational_preloads(connection)?;
        let started = Instant::now();
        if outcome.render_requested {
            self.rebuild_layout();
        }
        let load = self.text.borrow_mut().finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        if !outcome.render_requested && !force_accessibility_update {
            return Ok(Some(AdvanceResult::Runtime(Box::new(
                RendererRuntimeUpdate {
                    document: self.id,
                    clock_advanced: false,
                    next_timer_micros: self.next_timer_micros(),
                    runtime: runtime_report(
                        outcome,
                        self.script_runtime.is_some(),
                        self.media_runtime_report(),
                    ),
                    load,
                },
            ))));
        }
        self.presentation(outcome, style, load, connection)
            .map(Some)
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
