//! Native Win32 EDIT proposals, beforeinput verdicts, and DOM-authoritative rollback.

use super::*;

pub(super) fn attach_rejection(
    presentation: &mut Option<AdvanceResult>,
    rejection: NativeTextRejection,
) -> Result<(), String> {
    match presentation.as_mut() {
        Some(AdvanceResult::Presentation(presentation)) => {
            presentation.runtime.native_text_rejection = Some(rejection);
        }
        Some(AdvanceResult::Runtime(update)) => {
            update.runtime.native_text_rejection = Some(rejection);
        }
        _ => return Err("canceled native text edit produced no renderer report".into()),
    }
    Ok(())
}

impl DocumentRuntime {
    pub(super) fn native_text_input(
        &mut self,
        input: NativeTextInput,
    ) -> Result<Option<(ScriptOutcome, Option<NativeTextRejection>)>, String> {
        if input.generation != self.native_text_generation {
            return Ok(None);
        }
        let text = input.text;
        let Some(target) = self.resolve_target(text.target) else {
            return Ok(None);
        };
        let mut result = self.dispatch_user_input(UserInputEvent::NativeText {
            target: target.clone(),
            value: text.value.clone(),
            selection_start: text.selection_start,
            selection_end: text.selection_end,
            input_type: input.intent.input_type(),
            pre_selection: input.pre_selection,
        })?;
        if self.script_runtime.is_none() {
            self.accessibility_values
                .insert(target.id(), text.value.clone());
            if target.user_edit_input(&text.value) {
                request_state_render(&target, &mut result.outcome);
            }
        }
        let rejection = if let Some(rejected) = result.rejected_text {
            self.native_text_generation = self
                .native_text_generation
                .checked_add(1)
                .ok_or("native text generation overflow")?;
            self.accessibility_selection = Some((
                target.id(),
                rejected.selection_start,
                rejected.selection_end,
            ));
            Some(NativeTextRejection {
                sequence: text.sequence,
                generation: input.generation,
                target: text.target,
                value: rejected.value,
                selection_start: rejected.selection_start,
                selection_end: rejected.selection_end,
            })
        } else if result.default_allowed {
            self.accessibility_selection =
                Some((target.id(), text.selection_start, text.selection_end));
            None
        } else {
            return Err("native text edit did not return authoritative rollback state".into());
        };
        Ok(Some((result.outcome, rejection)))
    }
}
