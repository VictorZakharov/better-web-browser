//! Loading belongs to a CSS face, not its download URL. Distinct families,
//! descriptors and fallback lists can share a cached resource without sharing
//! FontFace status, loaded promises, or loading-event membership.
use super::{WebFont, WebFontFace, unicode_ranges::UnicodeRanges};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FontLoadIdentity {
    family: String,
    source: String,
    weight: u16,
    weight_min: u32,
    weight_max: u32,
    italic: bool,
    unicode_range: String,
    features: String,
    fallbacks: Vec<String>,
}

impl WebFontFace {
    pub(crate) fn loading_identity(&self) -> FontLoadIdentity {
        FontLoadIdentity {
            family: self.family.clone(),
            source: self.url.clone(),
            weight: self.weight,
            weight_min: self.weight_min.to_bits(),
            weight_max: self.weight_max.to_bits(),
            italic: self.italic,
            unicode_range: self.unicode_range.clone(),
            features: self.features.css_text(),
            fallbacks: self.fallback_urls.clone(),
        }
    }

    pub(crate) fn matches_loaded_css_font(&self, font: &WebFont) -> bool {
        font.script_source_id.is_none()
            && font.source_url == self.url
            && font.family == self.family
            && font.italic == self.italic
            && self.registered_weight(font.weight) == font.weight
            && font.features == self.features
            && UnicodeRanges::parse(&self.unicode_range).as_ref() == Some(&font.unicode_ranges)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face() -> WebFontFace {
        WebFontFace {
            family: "Identity".into(),
            url: "https://example.test/shared.ttf".into(),
            weight: 400,
            weight_min: 400.0,
            weight_max: 400.0,
            italic: false,
            unicode_range: "U+0-10FFFF".into(),
            features: Default::default(),
            fallback_urls: vec![],
        }
    }

    #[test]
    fn independent_descriptors_and_alternatives_never_share_loading_identity() {
        let original = face();
        let mut variants = Vec::new();
        let mut different = original.clone();
        different.family = "Other".into();
        variants.push(different);
        let mut different = original.clone();
        different.weight = 700;
        variants.push(different);
        let mut different = original.clone();
        different.weight_min = 100.0;
        variants.push(different);
        let mut different = original.clone();
        different.weight_max = 900.0;
        variants.push(different);
        let mut different = original.clone();
        different.italic = true;
        variants.push(different);
        let mut different = original.clone();
        different.unicode_range = "U+41".into();
        variants.push(different);
        let mut different = original.clone();
        different
            .fallback_urls
            .push("https://example.test/next.ttf".into());
        variants.push(different);
        let mut different = original.clone();
        different.url = "https://example.test/other.ttf".into();
        variants.push(different);
        for variant in variants {
            assert_ne!(original.loading_identity(), variant.loading_identity());
        }
        assert_eq!(
            original.loading_identity(),
            original.clone().loading_identity()
        );
    }

    #[test]
    fn loaded_resource_only_settles_a_matching_css_registration() {
        let original = face();
        let mut font = super::super::decode_web_font(
            &original,
            include_bytes!("../../../tests/canvas/fonts/ahem.ttf"),
        )
        .unwrap();
        assert!(original.matches_loaded_css_font(&font));
        let mut other = original.clone();
        other.family = "Unused".into();
        assert!(!other.matches_loaded_css_font(&font));
        other = original.clone();
        other.italic = true;
        assert!(!other.matches_loaded_css_font(&font));
        other = original.clone();
        other.weight = 700;
        other.weight_min = 700.0;
        other.weight_max = 700.0;
        assert!(!other.matches_loaded_css_font(&font));
        other = original.clone();
        other.unicode_range = "U+41".into();
        assert!(!other.matches_loaded_css_font(&font));
        font.script_source_id = Some(17);
        assert!(
            !original.matches_loaded_css_font(&font),
            "script membership cannot load a CSS face"
        );
    }
}
