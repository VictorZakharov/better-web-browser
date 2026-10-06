//! Fast renderer-local font discovery and CSS-family fallback.

use crate::engine::{FontSpec, WebFont};
use fontique::{
    Attributes, Blob, Collection, CollectionOptions, FallbackKey, FontInfoOverride, FontStyle,
    FontWeight, FontWidth, GenericFamily, QueryFamily, QueryFont, QueryStatus, Script, SourceCache,
};
use std::borrow::Cow;
use std::collections::HashMap;
use unicode_script::Script as UnicodeScript;
mod web_faces;

const MAX_SELECTIONS: usize = 4096;

#[derive(Hash, PartialEq, Eq)]
struct SelectionKey<'a> {
    family: Cow<'a, str>,
    cluster: Cow<'a, str>,
    script: u32,
    weight: u16,
    italic: bool,
}

impl SelectionKey<'_> {
    fn into_owned(self) -> SelectionKey<'static> {
        SelectionKey {
            family: Cow::Owned(self.family.into_owned()),
            cluster: Cow::Owned(self.cluster.into_owned()),
            script: self.script,
            weight: self.weight,
            italic: self.italic,
        }
    }
}

#[derive(Clone)]
pub(crate) struct SelectedFont {
    pub(crate) font: QueryFont,
    pub(crate) instance: FontInstanceKey,
    pub(crate) features: crate::engine::css::FontFeatures,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct FontInstanceKey {
    pub(crate) blob_id: u64,
    pub(crate) index: u32,
    pub(crate) weight: u16,
    pub(crate) italic: bool,
}

pub(crate) struct FontCatalog {
    collection: Collection,
    sources: SourceCache,
    registered_web_fonts: Vec<WebFont>,
    web_faces: web_faces::WebFaces,
    selections: HashMap<SelectionKey<'static>, SelectedFont>,
}

impl FontCatalog {
    pub(crate) fn new() -> Self {
        Self {
            // Fontique asks DirectWrite for family metadata lazily. On Windows this avoids the
            // eager scan of every installed font that dominated the previous cold path.
            collection: Collection::new(CollectionOptions::default()),
            sources: SourceCache::default(),
            registered_web_fonts: Vec::new(),
            web_faces: Default::default(),
            selections: HashMap::new(),
        }
    }

    pub(crate) fn register_web_fonts(&mut self, fonts: &[WebFont]) -> bool {
        if self.registered_web_fonts.len() == fonts.len()
            && self
                .registered_web_fonts
                .iter()
                .zip(fonts)
                .all(|(previous, next)| {
                    previous.family == next.family
                        && previous.weight == next.weight
                        && previous.italic == next.italic
                        && previous.unicode_ranges == next.unicode_ranges
                        && previous.features == next.features
                        && std::sync::Arc::ptr_eq(&previous.sfnt, &next.sfnt)
                })
        {
            return false;
        }
        *self = Self::new();
        self.web_faces = web_faces::WebFaces::register(&mut self.collection, fonts);
        self.registered_web_fonts = fonts.to_vec();
        true
    }

    pub(crate) fn reset_web_fonts(&mut self) -> bool {
        if self.registered_web_fonts.is_empty() {
            return false;
        }
        *self = Self::new();
        true
    }

    pub(crate) fn select(
        &mut self,
        family: &str,
        spec: &FontSpec,
        script: UnicodeScript,
        cluster: &str,
    ) -> Option<SelectedFont> {
        let key = SelectionKey {
            family: Cow::Borrowed(family),
            cluster: Cow::Borrowed(cluster),
            script: script.as_iso15924_tag(),
            weight: spec.weight,
            italic: spec.italic,
        };
        if let Some(cached) = self.selections.get(&key) {
            return Some(cached.clone());
        }
        let mut families = Vec::with_capacity(4);
        if cluster_looks_like_emoji(cluster) {
            families.push(QueryFamily::Generic(GenericFamily::Emoji));
        }
        use crate::engine::css::font_family::{self, Family};
        let requested = font_family::parse(family).unwrap_or_default();
        for family in &requested {
            match family {
                Family::Generic(name) => {
                    if let Some(generic) = GenericFamily::parse(name) {
                        families.push(QueryFamily::Generic(generic));
                    }
                }
                Family::Named(name) => families.push(QueryFamily::Named(name)),
            }
        }
        families.push(QueryFamily::Generic(GenericFamily::SansSerif));

        let attributes = Attributes::new(
            FontWidth::NORMAL,
            if spec.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            },
            FontWeight::new(spec.weight.clamp(1, 1000) as f32),
        );
        let script = Script::from_bytes(script.as_iso15924_tag().to_be_bytes());
        let mut query = self.collection.query(&mut self.sources);
        query.set_families(families);
        query.set_attributes(attributes);
        query.set_fallbacks(FallbackKey::new(script, None));

        let mut first = None;
        let mut selected = None;
        query.matches_with(|font| {
            let candidate = match self.web_faces.select(font.family.0, spec, cluster) {
                Some(Some(font)) => font,
                Some(None) => return QueryStatus::Continue,
                None => font.clone(),
            };
            first.get_or_insert_with(|| candidate.clone());
            if candidate
                .charmap()
                .is_some_and(|map| cluster_has_coverage(cluster, &map))
            {
                selected = Some(candidate);
                QueryStatus::Stop
            } else {
                QueryStatus::Continue
            }
        });
        let font = selected.or(first)?;
        let selected = SelectedFont {
            features: self.web_faces.features_for(&font),
            instance: FontInstanceKey {
                blob_id: font.blob.id(),
                index: font.index,
                weight: spec.weight,
                italic: spec.italic,
            },
            font,
        };
        if self.selections.len() >= MAX_SELECTIONS {
            self.selections.clear();
        }
        self.selections.insert(key.into_owned(), selected.clone());
        Some(selected)
    }

    #[cfg(test)]
    pub(crate) fn contains_family(&mut self, family: &str) -> bool {
        self.collection.family_by_name(family).is_some()
    }

    #[cfg(test)]
    pub(crate) fn first_system_font_bytes(&mut self) -> Option<Vec<u8>> {
        let family = self.collection.family_names().next()?.to_owned();
        let info = self.collection.family_by_name(&family)?;
        info.fonts()
            .first()?
            .load(Some(&mut self.sources))
            .map(|blob| blob.as_ref().to_vec())
    }
}

fn cluster_has_coverage(cluster: &str, map: &fontique::Charmap<'_>) -> bool {
    cluster
        .chars()
        .filter(|ch| !coverage_ignorable(*ch))
        .all(|ch| map.map(ch).is_some_and(|glyph| glyph != 0))
}

fn coverage_ignorable(ch: char) -> bool {
    matches!(
        ch,
        '\u{200c}'
            | '\u{200d}'
            | '\u{2060}'
            | '\u{fe0e}'
            | '\u{fe0f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2066}'..='\u{2069}'
    )
}

fn cluster_looks_like_emoji(cluster: &str) -> bool {
    cluster.chars().any(|ch| {
        matches!(
            ch as u32,
            0x1F000..=0x1FAFF | 0x2600..=0x27BF | 0xFE0F
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn repeated_clusters_reuse_selection_but_face_attributes_do_not_alias() {
        let mut catalog = FontCatalog::new();
        let mut spec = FontSpec {
            family: "sans-serif".into(),
            size: 16.0,
            weight: 400,
            italic: false,
            underline: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            rtl: false,
            kerning: true,
            variants: Default::default(),
            features: Default::default(),
        };
        let first = catalog
            .select(&spec.family, &spec, UnicodeScript::Latin, "A")
            .expect("system sans-serif font");
        assert_eq!(catalog.selections.len(), 1);
        let again = catalog
            .select(&spec.family, &spec, UnicodeScript::Latin, "A")
            .unwrap();
        assert_eq!(catalog.selections.len(), 1);
        assert_eq!(first.instance, again.instance);
        spec.weight = 700;
        catalog.select(&spec.family, &spec, UnicodeScript::Latin, "A");
        assert_eq!(catalog.selections.len(), 2);
        catalog.select(&spec.family, &spec, UnicodeScript::Latin, "B");
        assert_eq!(catalog.selections.len(), 3);
    }
}
