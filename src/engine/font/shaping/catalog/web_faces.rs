//! A CSS composite face can contain multiple equal-style font files with
//! disjoint unicode-ranges. Fontique's family query supplies family ordering;
//! retain its registered FontInfo objects so coverage can choose a real subset
//! rather than repeatedly accepting only the family's first equal-style file.
use super::*;
use crate::engine::font::{WebFontFace, unicode_ranges::UnicodeRanges};
use fontique::{FamilyId, FontInfo};

#[derive(Default)]
pub(super) struct WebFaces {
    faces: Vec<Candidate>,
}

struct Candidate {
    face: WebFontFace,
    ranges: UnicodeRanges,
    info: FontInfo,
    font: QueryFont,
    features: crate::engine::css::FontFeatures,
}

impl WebFaces {
    pub(super) fn features_for(&self, font: &QueryFont) -> crate::engine::css::FontFeatures {
        self.faces
            .iter()
            .find(|candidate| {
                candidate.font.blob.id() == font.blob.id() && candidate.font.index == font.index
            })
            .map(|candidate| candidate.features.clone())
            .unwrap_or_default()
    }
    pub(super) fn register(collection: &mut Collection, fonts: &[WebFont]) -> Self {
        let mut result = Self::default();
        for font in fonts {
            let blob = Blob::new(std::sync::Arc::new(font.sfnt.clone()));
            let style = if font.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            };
            let groups = collection.register_fonts(
                blob.clone(),
                Some(FontInfoOverride {
                    family_name: Some(&font.family),
                    style: Some(style),
                    weight: Some(FontWeight::new(font.weight.clamp(1, 1000) as f32)),
                    ..FontInfoOverride::default()
                }),
            );
            for (family, infos) in groups {
                for info in infos {
                    result.faces.push(Candidate {
                        face: WebFontFace {
                            family: font.family.clone(),
                            weight: font.weight,
                            weight_min: f32::from(font.weight),
                            weight_max: f32::from(font.weight),
                            italic: font.italic,
                            url: font.source_url.clone(),
                            fallback_urls: Vec::new(),
                            unicode_range: font.unicode_ranges.serialize(),
                            features: font.features.clone(),
                        },
                        ranges: font.unicode_ranges.clone(),
                        features: font.features.clone(),
                        font: QueryFont {
                            family: (family, 0),
                            blob: blob.clone(),
                            index: info.index(),
                            synthesis: Default::default(),
                            charmap_index: info.charmap_index(),
                        },
                        info,
                    });
                }
            }
        }
        // FamilyInfo may reorder fonts by attributes as later registrations arrive.
        // Resolve slots after the entire immutable registry has been assembled.
        for candidate in &mut result.faces {
            if let Some(family) = collection.family(candidate.font.family.0)
                && let Some(index) = family.fonts().iter().position(|font| {
                    font.source().id() == candidate.info.source().id()
                        && font.index() == candidate.info.index()
                })
            {
                candidate.font.family.1 = index;
            }
        }
        result
    }

    /// Outer None means a system family; Some(None) means a downloadable family
    /// exists but none of its best-style subsets covers this cluster.
    pub(super) fn select(
        &self,
        family: FamilyId,
        spec: &FontSpec,
        cluster: &str,
    ) -> Option<Option<QueryFont>> {
        let rank = |candidate: &Candidate| {
            let weight = candidate.face.weight_match_rank(spec.weight);
            (
                u8::from(candidate.face.italic != spec.italic),
                weight.0,
                weight.1,
            )
        };
        let best = self
            .faces
            .iter()
            .filter(|candidate| candidate.font.family.0 == family)
            .map(rank)
            .min_by(|a, b| a.partial_cmp(b).unwrap())?;
        let selected = self
            .faces
            .iter()
            .rev()
            .filter(|candidate| candidate.font.family.0 == family && rank(candidate) == best)
            .find(|candidate| {
                cluster
                    .chars()
                    .filter(|character| !coverage_ignorable(*character))
                    .all(|character| candidate.ranges.contains(character as u32))
                    && candidate
                        .font
                        .charmap()
                        .is_some_and(|map| cluster_has_coverage(cluster, &map))
            })
            .map(|candidate| {
                let mut font = candidate.font.clone();
                font.synthesis = candidate.info.synthesis(
                    FontWidth::NORMAL,
                    if spec.italic {
                        FontStyle::Italic
                    } else {
                        FontStyle::Normal
                    },
                    FontWeight::new(spec.weight.clamp(1, 1000) as f32),
                );
                font
            });
        Some(selected)
    }
}

#[cfg(test)]
mod tests;
