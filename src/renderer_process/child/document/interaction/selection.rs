//! Native caret/range moves do not imply a text edit or input/change event.

use super::*;
use crate::renderer_protocol::TextSelectionInput;

impl DocumentRuntime {
    pub(super) fn selection_input(
        &mut self,
        input: TextSelectionInput,
    ) -> Result<ScriptOutcome, String> {
        let Some(target) = self.resolve_target(input.target) else {
            return Ok(ScriptOutcome::default());
        };
        let selectable = match target.tag_name() {
            Some("textarea") => true,
            Some("input") => matches!(
                target.input_state_name().as_str(),
                "text" | "search" | "tel" | "url" | "password"
            ),
            _ => false,
        };
        if !selectable {
            return Ok(ScriptOutcome::default());
        }
        let is_native_text_control = self.layout.items.iter().any(|item| {
            matches!(item, DisplayItem::Control(spec)
                if spec.node_id == target.id() && !spec.authored_content && matches!(spec.kind,
                    ControlKind::Text | ControlKind::Search | ControlKind::Password | ControlKind::TextArea))
        });
        if !is_native_text_control {
            return Ok(ScriptOutcome::default());
        }
        let length = match target.tag_name() {
            Some("textarea") => target.textarea_api_value().encode_utf16().count(),
            Some("input") => target.input_value().encode_utf16().count(),
            _ => return Ok(ScriptOutcome::default()),
        };
        if input.selection_end as usize > length {
            return Ok(ScriptOutcome::default());
        }
        self.accessibility_selection =
            Some((target.id(), input.selection_start, input.selection_end));
        Ok(self
            .dispatch_user_input(UserInputEvent::Selection {
                target,
                selection_start: input.selection_start,
                selection_end: input.selection_end,
                direction: input.direction,
            })?
            .outcome)
    }
}
