//! CSS-connected fonts use the same live owners, imports and adopted sheets as
//! the cascade, rather than the resource loader's completion-order snapshot.
use super::*;

impl StyleSet {
    pub(crate) fn document_font_faces(&self) -> Vec<crate::engine::font::WebFontFace> {
        self.compiled.font_faces.clone()
    }
}

pub(super) fn collect(
    inputs: &[Rc<SheetInput>],
    environment: MediaEnvironment,
) -> Vec<crate::engine::font::WebFontFace> {
    inputs
        .iter()
        .filter(|input| matches!(input.scope, RuleScope::Document | RuleScope::Shadow(_)))
        .flat_map(|input| {
            crate::engine::css::stylesheet::font_faces::collect(
                &input.source,
                &input.base_url,
                environment,
            )
        })
        .take(64)
        .collect()
}
