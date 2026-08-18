//! Fast renderer-local font discovery and CSS-family fallback.

use crate::engine::{FontSpec, WebFont};
use fontique::{
    Attributes, Blob, Collection, CollectionOptions, FallbackKey, FontInfoOverride, FontStyle,
    FontWeight, FontWidth, GenericFamily, QueryFamily, QueryFont, QueryStatus, Script, SourceCache,
};
use unicode_script::Script as UnicodeScript;

#[derive(Clone)]
pub(super) struct SelectedFont {
    pub(super) font: QueryFont,
    pub(super) instance: FontInstanceKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FontInstanceKey {
    pub(super) blob_id: u64,
    pub(super) index: u32,
    pub(super) weight: u16,
    pub(super) italic: bool,
}

pub(super) struct FontCatalog {
    collection: Collection,
    sources: SourceCache,
    registered_web_fonts: usize,
}

impl FontCatalog {
    pub(super) fn new() -> Self {
        Self {
            // Fontique asks DirectWrite for family metadata lazily. On Windows this avoids the
            // eager scan of every installed font that dominated the previous cold path.
            collection: Collection::new(CollectionOptions::default()),
            sources: SourceCache::default(),
            registered_web_fonts: 0,
        }
    }

    pub(super) fn register_web_fonts(&mut self, fonts: &[WebFont]) -> bool {
        if self.registered_web_fonts == fonts.len() {
            return false;
        }
        *self = Self::new();
        for font in fonts {
            let style = if font.italic {
                FontStyle::Italic
            } else {
                FontStyle::Normal
            };
            self.collection.register_fonts(
                Blob::from(font.sfnt.clone()),
                Some(FontInfoOverride {
                    family_name: Some(&font.family),
                    style: Some(style),
                    weight: Some(FontWeight::new(font.weight.clamp(1, 1000) as f32)),
                    ..FontInfoOverride::default()
                }),
            );
        }
        self.registered_web_fonts = fonts.len();
        true
    }

    pub(super) fn reset_web_fonts(&mut self) -> bool {
        if self.registered_web_fonts == 0 {
            return false;
        }
        *self = Self::new();
        true
    }

    pub(super) fn select(
        &mut self,
        family: &str,
        spec: &FontSpec,
        script: UnicodeScript,
        cluster: &str,
    ) -> Option<SelectedFont> {
        let mut families = Vec::with_capacity(4);
        if cluster_looks_like_emoji(cluster) {
            families.push(QueryFamily::Generic(GenericFamily::Emoji));
        }
        for family in css_families(family) {
            match GenericFamily::parse(&family.to_ascii_lowercase()) {
                Some(generic) => families.push(QueryFamily::Generic(generic)),
                None => families.push(QueryFamily::Named(family)),
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
            first.get_or_insert_with(|| font.clone());
            if font
                .charmap()
                .is_some_and(|map| cluster_has_coverage(cluster, &map))
            {
                selected = Some(font.clone());
                QueryStatus::Stop
            } else {
                QueryStatus::Continue
            }
        });
        let font = selected.or(first)?;
        Some(SelectedFont {
            instance: FontInstanceKey {
                blob_id: font.blob.id(),
                index: font.index,
                weight: spec.weight,
                italic: spec.italic,
            },
            font,
        })
    }

    #[cfg(test)]
    pub(super) fn contains_family(&mut self, family: &str) -> bool {
        self.collection.family_by_name(family).is_some()
    }

    #[cfg(test)]
    pub(super) fn first_system_font_bytes(&mut self) -> Option<Vec<u8>> {
        let family = self.collection.family_names().next()?.to_owned();
        let info = self.collection.family_by_name(&family)?;
        info.fonts()
            .first()?
            .load(Some(&mut self.sources))
            .map(|blob| blob.as_ref().to_vec())
    }
}

fn css_families(value: &str) -> impl Iterator<Item = &str> {
    value.split(',').filter_map(|family| {
        let family = family.trim().trim_matches(['\'', '"']);
        (!family.is_empty()).then_some(family)
    })
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
