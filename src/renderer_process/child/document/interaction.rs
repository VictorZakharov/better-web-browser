//! Renderer-owned hit testing, DOM event dispatch, default actions, and input sequencing.

mod default_actions;

use super::*;
use crate::engine::dom::{NodeId, NodeRef};
use crate::engine::{ControlKind, ControlSpec, DisplayItem, UserInputEvent, UserInputModifiers};
use crate::renderer_protocol::{
    DocumentInput, DocumentLifecycle, DocumentNodeId, InputModifiers, KeyPhase,
    NavigationDisposition, PointerButton, PointerCursor, PointerCursorResult, PointerInput,
    PointerPhase, PresentationAcknowledgement,
};

pub(in crate::renderer_process::child) struct InteractionResult {
    pub(in crate::renderer_process::child) presentation: Option<RendererPresentation>,
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
        let force_accessibility_update =
            matches!(&input, DocumentInput::Text(_) | DocumentInput::Focus(_));
        let mut cursor = None;
        let (mut outcome, navigation) = match input {
            DocumentInput::Pointer(input) => {
                let interaction = self.pointer_input(input, connection)?;
                cursor = interaction.cursor;
                (interaction.outcome, interaction.navigation)
            }
            DocumentInput::Keyboard(input) => {
                let key_code = key_code(&input.key);
                let activates_form = input.phase == KeyPhase::Down && input.key == "Enter";
                let target = input
                    .target
                    .and_then(|target| self.resolve_target(target))
                    .or_else(|| self.focused_node.and_then(|id| self.page.dom.find_node(id)));
                let target_id = target.as_ref().map(|node| node.id());
                let result = self.dispatch_user_input(
                    UserInputEvent::Keyboard {
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
                    },
                    connection,
                )?;
                let mut outcome = result.outcome;
                let navigation = if activates_form && result.default_allowed {
                    self.keyboard_default_action(target_id, connection, &mut outcome)?
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
                let result = self.dispatch_user_input(
                    UserInputEvent::Text {
                        target,
                        value: input.value,
                        selection_start: input.selection_start,
                        selection_end: input.selection_end,
                    },
                    connection,
                )?;
                (result.outcome, None)
            }
            DocumentInput::Focus(input) => {
                let target = input.target.and_then(|target| self.resolve_target(target));
                self.focused_node = input
                    .focused
                    .then(|| target.as_ref().map(|node| node.id()))
                    .flatten();
                let result = self.dispatch_user_input(
                    UserInputEvent::Focus {
                        target,
                        focused: input.focused,
                    },
                    connection,
                )?;
                (result.outcome, None)
            }
            DocumentInput::Scroll(input) => {
                let result = self.dispatch_user_input(
                    UserInputEvent::Scroll {
                        x: input.x,
                        y: input.y,
                    },
                    connection,
                )?;
                (result.outcome, None)
            }
            DocumentInput::Lifecycle(input) => {
                let previous = lifecycle_name(self.lifecycle);
                self.lifecycle = input.state;
                let result = self.dispatch_user_input(
                    UserInputEvent::Lifecycle {
                        state: lifecycle_name(input.state),
                        previous,
                    },
                    connection,
                )?;
                (result.outcome, None)
            }
        };
        self.admit_user_input_outcome(&mut outcome, connection)?;
        let presentation =
            self.presentation_after_user_input(outcome, force_accessibility_update)?;
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

    fn pointer_input(
        &mut self,
        input: PointerInput,
        connection: &mut ChildConnection,
    ) -> Result<PointerInteraction, String> {
        let target = input
            .target
            .and_then(|target| self.explicit_target(target))
            .or_else(|| self.hit_target(input.x, input.y));
        let cursor = (input.phase == PointerPhase::Move).then_some(PointerCursorResult {
            document: self.id,
            sequence: input.sequence,
            cursor: cursor_for_target(target.as_ref()),
        });
        let target_id = target.as_ref().map(|target| target.node.id());
        let activate = match input.phase {
            PointerPhase::Down => {
                self.pointer_down = target_id.map(|target| (target, input.button));
                false
            }
            PointerPhase::Up => {
                self.pointer_down.take() == target_id.map(|id| (id, input.button))
                    && matches!(input.button, PointerButton::Primary | PointerButton::Middle)
            }
            PointerPhase::Activate => {
                matches!(input.button, PointerButton::Primary | PointerButton::Middle)
            }
            PointerPhase::Move => false,
        };
        let result = self.dispatch_user_input(
            UserInputEvent::Pointer {
                target: target.as_ref().map(|target| target.node.clone()),
                phase: match input.phase {
                    PointerPhase::Move => "move",
                    PointerPhase::Down => "down",
                    PointerPhase::Up => "up",
                    PointerPhase::Activate => "activate",
                },
                button: dom_button(input.button),
                buttons: if matches!(input.phase, PointerPhase::Down) {
                    dom_buttons(input.button)
                } else {
                    0
                },
                x: input.x,
                y: input.y,
                activate,
                modifiers: input.modifiers.into(),
            },
            connection,
        )?;
        let mut outcome = result.outcome;
        let navigation = if activate && result.default_allowed {
            self.pointer_default_action(target.as_ref(), input, connection, &mut outcome)?
        } else {
            None
        };
        Ok(PointerInteraction {
            outcome,
            navigation,
            cursor,
        })
    }

    pub(super) fn dispatch_user_input(
        &mut self,
        event: UserInputEvent,
        connection: &mut ChildConnection,
    ) -> Result<crate::engine::UserInputResult, String> {
        let Some(runtime) = self.script_runtime.as_mut() else {
            return Ok(crate::engine::UserInputResult {
                default_allowed: true,
                ..Default::default()
            });
        };
        let document = self.id;
        let mut loader = |url: &str, kind, options| {
            fetch_script_source(connection, document, url, kind, options)
        };
        Ok(runtime.dispatch_user_input_with_loader(event, Some(&mut loader)))
    }

    pub(super) fn admit_user_input_outcome(
        &mut self,
        outcome: &mut ScriptOutcome,
        connection: &mut ChildConnection,
    ) -> Result<(), String> {
        self.pending_fetches.append(&mut outcome.fetch_actions);
        self.pending_worker_actions
            .append(&mut outcome.worker_actions);
        connection.send_state_mutations(self.id, outcome)
    }

    fn presentation_after_user_input(
        &mut self,
        outcome: ScriptOutcome,
        force_accessibility_update: bool,
    ) -> Result<Option<RendererPresentation>, String> {
        let needs_present = force_accessibility_update
            || outcome.render_requested
            || outcome.executed > 0
            || !outcome.errors.is_empty()
            || !outcome.console.is_empty()
            || !outcome.diagnostics.is_empty()
            || outcome.navigation_url.is_some()
            || !outcome.cookie_updates.is_empty();
        if !needs_present {
            return Ok(None);
        }
        let style = if outcome.render_requested {
            self.page.refresh_resources_after_invalidation(
                self.viewport.style_width,
                &outcome.invalidation,
            )
        } else {
            StyleRefreshStats::default()
        };
        let started = Instant::now();
        if outcome.render_requested {
            self.rebuild_layout();
        }
        let load = self.text.finish_load_report(PageLoadReport {
            layout_micros: micros(started.elapsed()),
            ..PageLoadReport::default()
        });
        self.presentation(outcome, style, load).map(Some)
    }

    fn resolve_target(&self, target: DocumentNodeId) -> Option<NodeRef> {
        NodeId::from_wire(target.get()).and_then(|id| self.page.dom.find_node(id))
    }

    fn explicit_target(&self, target: DocumentNodeId) -> Option<HitTarget> {
        let node = self.resolve_target(target)?;
        self.layout.items.iter().find_map(|item| match item {
            DisplayItem::Control(control) if control.node_id == node.id() => Some(HitTarget {
                node: node.clone(),
                link: None,
                control: Some((**control).clone()),
            }),
            DisplayItem::Text {
                node_id: Some(node_id),
                link: Some(link),
                ..
            } if *node_id == node.id() => Some(HitTarget {
                node: node.clone(),
                link: Some(link.clone()),
                control: None,
            }),
            _ => None,
        })
    }

    fn hit_target(&self, x: f32, y: f32) -> Option<HitTarget> {
        self.layout.items.iter().rev().find_map(|item| match item {
            DisplayItem::Text {
                rect,
                link: Some(link),
                node_id: Some(node_id),
                ..
            } if contains(*rect, x, y) => self.page.dom.find_node(*node_id).map(|node| HitTarget {
                node,
                link: Some(link.clone()),
                control: None,
            }),
            DisplayItem::Control(control) if contains(control.rect, x, y) => self
                .page
                .dom
                .find_node(control.node_id)
                .map(|node| HitTarget {
                    node,
                    link: None,
                    control: Some((**control).clone()),
                }),
            _ => None,
        })
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

fn dom_button(button: PointerButton) -> u8 {
    match button {
        PointerButton::Primary | PointerButton::None => 0,
        PointerButton::Middle => 1,
        PointerButton::Secondary => 2,
    }
}

fn dom_buttons(button: PointerButton) -> u8 {
    match button {
        PointerButton::None => 0,
        PointerButton::Primary => 1,
        PointerButton::Secondary => 2,
        PointerButton::Middle => 4,
    }
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
        _ => key
            .chars()
            .next()
            .filter(|_| key.chars().count() == 1)
            .map_or(0, |c| c as u32),
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
}
