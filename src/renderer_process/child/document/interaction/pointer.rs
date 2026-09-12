//! Pointer hit testing, button ownership and activation within one document.
use super::*;

impl DocumentRuntime {
    pub(super) fn pointer_input(
        &mut self,
        input: PointerInput,
    ) -> Result<PointerInteraction, String> {
        let target = (input.phase != PointerPhase::Leave)
            .then(|| {
                input
                    .target
                    .and_then(|target| self.explicit_target(target))
                    .or_else(|| self.hit_target(input.x, input.y))
            })
            .flatten();
        let cursor = (input.phase == PointerPhase::Move).then_some(PointerCursorResult {
            document: self.id,
            sequence: input.sequence,
            cursor: cursor_for_target(target.as_ref()),
        });
        let target_id = target.as_ref().map(|target| target.node.id());
        let button_index = dom_button(input.button) as usize;
        if input.phase != PointerPhase::Activate {
            // State snapshots also observe a button released outside the content surface.
            // Keep only a matching current release long enough to determine activation.
            for (index, mask) in [1, 4, 2].into_iter().enumerate() {
                if input.buttons & mask == 0
                    && !(input.phase == PointerPhase::Up && button_index == index)
                {
                    self.pointer_down[index] = None;
                }
            }
        }
        let activate = match input.phase {
            PointerPhase::Down => {
                if input.button != PointerButton::None {
                    self.pointer_down[button_index] = target_id;
                }
                false
            }
            PointerPhase::Up => {
                self.pointer_down[button_index]
                    .take()
                    .is_some_and(|id| Some(id) == target_id)
                    && matches!(input.button, PointerButton::Primary | PointerButton::Middle)
            }
            PointerPhase::Activate => {
                matches!(input.button, PointerButton::Primary | PointerButton::Middle)
            }
            PointerPhase::Move | PointerPhase::Leave => false,
        };
        let result = self.dispatch_user_input(UserInputEvent::Pointer {
            target: target.as_ref().map(|target| target.node.clone()),
            phase: match input.phase {
                PointerPhase::Move => "move",
                PointerPhase::Leave => "leave",
                PointerPhase::Down => "down",
                PointerPhase::Up => "up",
                PointerPhase::Activate => "activate",
            },
            button: dom_button(input.button),
            buttons: input.buttons,
            x: input.x,
            y: input.y,
            activate,
            modifiers: input.modifiers.into(),
        })?;
        let mut outcome = result.outcome;
        if self.script_runtime.is_none() {
            let boundary = crate::engine::dom::Node::update_hover_path(
                &mut self.scriptless_pointer_path,
                target.as_ref().map(|target| target.node.clone()),
            );
            if !boundary.entering.is_empty() || !boundary.leaving.is_empty() {
                outcome.render_requested = true;
                outcome.invalidation = crate::engine::invalidation::RenderInvalidation {
                    roots: vec![self.page.dom.document.id()],
                    impact: crate::engine::invalidation::MutationKind::State.impact(),
                    mutation_count: 0,
                    rebuild_style_rules: false,
                    removed_nodes: Vec::new(),
                };
            }
        }
        let navigation = if activate && result.default_allowed {
            self.pointer_default_action(target.as_ref(), input, &mut outcome)?
        } else {
            None
        };
        Ok(PointerInteraction {
            outcome,
            navigation,
            cursor,
        })
    }
}

fn dom_button(button: PointerButton) -> u8 {
    match button {
        PointerButton::Primary | PointerButton::None => 0,
        PointerButton::Middle => 1,
        PointerButton::Secondary => 2,
    }
}
