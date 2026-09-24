//! Install/remove script-created FontFace resources before the next style/layout checkpoint.

use super::*;
use crate::engine::script::ScriptFontAction;

impl DocumentRuntime {
    pub(super) fn apply_font_actions(&mut self, outcome: &mut ScriptOutcome) {
        let mut changed = false;
        for action in outcome.font_actions.drain(..) {
            match action {
                ScriptFontAction::Add { id, font } => {
                    if font.script_source_id != Some(id) {
                        continue;
                    }
                    if let Some(existing) = self
                        .page
                        .fonts
                        .iter_mut()
                        .find(|existing| existing.script_source_id == Some(id))
                    {
                        *existing = font;
                        changed = true;
                    } else if self.page.fonts.len() < crate::limits::MAX_WEB_FONTS {
                        self.page.fonts.push(font);
                        changed = true;
                    } else {
                        outcome
                            .diagnostics
                            .push("FontFaceSet webfont limit reached".into());
                    }
                }
                ScriptFontAction::Remove { id } => {
                    let before = self.page.fonts.len();
                    self.page
                        .fonts
                        .retain(|font| font.script_source_id != Some(id));
                    changed |= self.page.fonts.len() != before;
                }
            }
        }
        if changed {
            let mut text = self.text.borrow_mut();
            text.invalidate_web_fonts();
            text.register_web_fonts(&self.page.fonts);
            outcome.render_requested = true;
            outcome.invalidation =
                crate::engine::invalidation::RenderInvalidation::full(self.page.dom.document.id());
        }
    }
}
