//! Reusable OpenType shape plans.
//!
//! Compiling a plan (feature map, lookups, script shaper) costs more than shaping a short run, and
//! `Shaper::shape` compiles one per call. A page shapes thousands of runs with a handful of
//! configurations, so plans are kept per face and found with HarfRust's own `ShapePlanKey`,
//! which compares everything a plan depends on: script, direction, language, the variation
//! instance's feature variations, and the user features.

use harfrust::{Feature, ShapePlan, ShapePlanKey, Shaper, ShaperInstance, UnicodeBuffer};
use std::collections::HashMap;

/// Plans kept per face; a face rarely sees more configurations, and the oldest goes first.
const PLANS_PER_FACE: usize = 16;

#[derive(Default)]
pub(super) struct ShapePlans(pub(super) HashMap<(u64, u32), Vec<ShapePlan>>);

impl ShapePlans {
    pub(super) fn clear(&mut self) {
        self.0.clear();
    }

    /// The plan for shaping `buffer` (whose segment properties are already set) with `shaper`.
    pub(super) fn get(
        &mut self,
        face: (u64, u32),
        shaper: &Shaper,
        instance: &ShaperInstance,
        buffer: &UnicodeBuffer,
        features: &[Feature],
    ) -> &ShapePlan {
        let script = Some(buffer.script()).filter(|script| *script != harfrust::script::UNKNOWN);
        let language = buffer.language();
        let key = ShapePlanKey::new(script, buffer.direction())
            .language(language.as_ref())
            .instance(Some(instance))
            .features(features);
        let plans = self.0.entry(face).or_default();
        let index = match plans.iter().position(|plan| key.matches(plan)) {
            Some(index) => index,
            None => {
                if plans.len() == PLANS_PER_FACE {
                    plans.remove(0);
                }
                plans.push(ShapePlan::new(
                    shaper,
                    buffer.direction(),
                    script,
                    language.as_ref(),
                    features,
                ));
                plans.len() - 1
            }
        };
        &plans[index]
    }
}

#[cfg(test)]
mod tests {
    use super::super::{FontCatalog, TextShaper};
    use crate::engine::FontSpec;

    fn spec(letter_spacing: f32) -> FontSpec {
        FontSpec {
            family: "Arial".into(),
            size: 16.0,
            weight: 400,
            italic: false,
            underline: false,
            letter_spacing,
            word_spacing: 0.0,
            rtl: false,
            kerning: true,
            variants: Default::default(),
            features: Default::default(),
        }
    }

    fn glyphs(shaper: &mut TextShaper, catalog: &mut FontCatalog, spec: &FontSpec) -> Vec<u16> {
        let output = shaper.shape(catalog, "office flight", spec);
        output.glyphs.iter().map(|glyph| glyph.glyph_id).collect()
    }

    fn plan_count(shaper: &TextShaper) -> usize {
        shaper.plans.0.values().map(Vec::len).sum()
    }

    #[test]
    fn runs_with_one_configuration_share_a_plan_and_shape_alike() {
        let mut catalog = FontCatalog::new();
        let mut shaper = TextShaper::default();
        let first = glyphs(&mut shaper, &mut catalog, &spec(0.0));
        assert_eq!(plan_count(&shaper), 1);
        assert_eq!(glyphs(&mut shaper, &mut catalog, &spec(0.0)), first);
        assert_eq!(plan_count(&shaper), 1);
        // Letter spacing turns optional ligatures off: a different feature set needs its own plan.
        let spaced = glyphs(&mut shaper, &mut catalog, &spec(1.0));
        assert_eq!(plan_count(&shaper), 2);
        assert_eq!(glyphs(&mut shaper, &mut catalog, &spec(0.0)), first);
        assert_eq!(glyphs(&mut shaper, &mut catalog, &spec(1.0)), spaced);
        assert_eq!(plan_count(&shaper), 2);
        shaper.clear();
        assert_eq!(plan_count(&shaper), 0);
    }
}
