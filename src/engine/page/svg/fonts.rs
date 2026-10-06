//! SVG delegates shaping/outline conversion to usvg and family selection to
//! Breeze's existing lazy Fontique catalog. No font-directory scan or URL I/O.
use crate::engine::font::WebFont;
use resvg::usvg;
use std::cell::OnceCell;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

/// One immutable font input shared by all SVGs in a page refresh. Content is
/// hashed lazily once, not once per SVG; no allocation-address cache keys.
pub(super) struct Input {
    fonts: Arc<[WebFont]>,
    version: OnceCell<u64>,
}

impl Input {
    pub(super) fn new(fonts: &[WebFont]) -> Self {
        Self {
            fonts: fonts.to_vec().into(),
            version: OnceCell::new(),
        }
    }

    pub(super) fn version(&self) -> u64 {
        *self.version.get_or_init(|| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            for font in self.fonts.iter() {
                font.family.hash(&mut hash);
                font.weight.hash(&mut hash);
                font.italic.hash(&mut hash);
                font.unicode_ranges.serialize().hash(&mut hash);
                font.features.css_text().hash(&mut hash);
                font.sfnt.hash(&mut hash);
            }
            hash.finish()
        })
    }

    pub(super) fn snapshot(&self) -> Arc<[WebFont]> {
        self.fonts.clone()
    }
}

#[cfg(test)]
mod input_tests {
    use super::*;

    fn font() -> WebFont {
        WebFont {
            family: "Snapshot".into(),
            weight: 400,
            italic: false,
            sfnt: vec![17; 1024].into(),
            source_url: "test-font:input".into(),
            script_source_id: None,
            unicode_ranges: Default::default(),
            features: Default::default(),
        }
    }

    #[test]
    fn font_input_is_lazy_shared_and_immutable_after_source_replacement() {
        let mut source = font();
        let input = Input::new(std::slice::from_ref(&source));
        assert!(input.version.get().is_none());
        assert!(Arc::ptr_eq(&input.snapshot(), &input.snapshot()));
        let stamp = input.version();
        assert_eq!(input.version.get(), Some(&stamp));
        source.sfnt = vec![19; 1024].into();
        assert_eq!(input.version(), stamp);
        assert_eq!(input.snapshot()[0].sfnt[0], 17);
        assert_ne!(Input::new(&[source]).version(), stamp);
    }

    #[test]
    fn font_content_and_matching_descriptors_are_part_of_the_stamp() {
        let original = font();
        let stamp = Input::new(std::slice::from_ref(&original)).version();
        let mut changed = original.clone();
        changed.family.push('X');
        assert_ne!(Input::new(&[changed]).version(), stamp);
        let mut changed = original.clone();
        changed.weight = 700;
        assert_ne!(Input::new(&[changed]).version(), stamp);
        let mut changed = original.clone();
        changed.italic = true;
        assert_ne!(Input::new(&[changed]).version(), stamp);
        let mut changed = original.clone();
        changed.unicode_ranges =
            crate::engine::font::unicode_ranges::UnicodeRanges::parse("U+41").unwrap();
        assert_ne!(Input::new(&[changed]).version(), stamp);
        let mut changed = original;
        changed.features = crate::engine::css::FontFeatures::parse("'liga' off").unwrap();
        assert_ne!(Input::new(&[changed]).version(), stamp);
    }
}

pub(super) fn configure(options: &mut usvg::Options<'static>, fonts: &[WebFont]) {
    #[cfg(windows)]
    {
        options.font_resolver = windows::resolver(fonts);
    }
    #[cfg(not(windows))]
    {
        let database = std::sync::Arc::make_mut(&mut options.fontdb);
        for font in fonts {
            database.load_font_source(usvg::fontdb::Source::Binary(std::sync::Arc::new(
                font.sfnt.clone(),
            )));
        }
    }
}

#[cfg(windows)]
mod windows {
    use super::*;
    use crate::engine::FontSpec;
    use crate::engine::font::shaping::{FontCatalog, SelectedFont};
    use crate::limits::MAX_FONT_BYTES;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use unicode_script::{Script, UnicodeScript};
    use usvg::fontdb::{Database, ID, Source};

    struct State {
        catalog: Option<FontCatalog>,
        fonts: Vec<WebFont>,
        ids: HashMap<(u64, u32), ID>,
        origins: HashMap<ID, FontSpec>,
        loaded_bytes: usize,
    }

    pub(super) fn resolver(fonts: &[WebFont]) -> usvg::FontResolver<'static> {
        let state = Arc::new(Mutex::new(State {
            catalog: None,
            fonts: fonts.to_vec(),
            ids: HashMap::new(),
            origins: HashMap::new(),
            loaded_bytes: 0,
        }));
        let selection = state.clone();
        usvg::FontResolver {
            select_font: Box::new(move |font, database| {
                let family = family_list(font.families());
                let spec = specification(
                    family,
                    font.weight(),
                    font.style() != usvg::FontStyle::Normal,
                );
                selection
                    .lock()
                    .ok()?
                    .select(&spec, Script::Latin, " ", None, database)
            }),
            select_fallback: Box::new(move |character, excluded, database| {
                let base = excluded.first().and_then(|id| database.face(*id));
                let weight = base.map_or(400, |face| face.weight.0);
                let italic = base.is_some_and(|face| face.style != usvg::fontdb::Style::Normal);
                let family = base
                    .and_then(|face| face.families.first())
                    .map_or("sans-serif", |family| family.0.as_str());
                let mut state = state.lock().ok()?;
                let spec = excluded
                    .first()
                    .and_then(|id| state.origins.get(id))
                    .cloned()
                    .unwrap_or_else(|| specification(family.to_owned(), weight, italic));
                let cluster = character.to_string();
                let id = state.select(
                    &spec,
                    character.script(),
                    &cluster,
                    Some(character),
                    database,
                )?;
                (!excluded.contains(&id)).then_some(id)
            }),
        }
    }

    fn family_list(families: &[svgtypes::FontFamily]) -> String {
        let mut output = String::new();
        for family in families {
            if !output.is_empty() {
                output.push(',');
            }
            match family {
                svgtypes::FontFamily::Named(name) => {
                    // Names containing commas or quotes remain one CSS family.
                    cssparser::serialize_string(name, &mut output).expect("String formatting");
                }
                generic => output.push_str(&generic.to_string()),
            }
        }
        output
    }

    fn specification(family: String, weight: u16, italic: bool) -> FontSpec {
        FontSpec {
            family,
            size: 16.0,
            weight,
            italic,
            underline: false,
            letter_spacing: 0.0,
            word_spacing: 0.0,
            rtl: false,
            kerning: true,
            features: Default::default(),
            variants: Default::default(),
        }
    }

    impl State {
        fn select(
            &mut self,
            spec: &FontSpec,
            script: Script,
            cluster: &str,
            required: Option<char>,
            database: &mut Arc<Database>,
        ) -> Option<ID> {
            let catalog = self.catalog.get_or_insert_with(|| {
                let mut catalog = FontCatalog::new();
                catalog.register_web_fonts(&self.fonts);
                catalog
            });
            let selected = catalog.select(&spec.family, spec, script, cluster)?;
            // Fontique may return its first candidate when no candidate covers
            // the character. Do not claim that candidate as a real SVG fallback.
            if let Some(character) = required
                && selected
                    .font
                    .charmap()
                    .is_none_or(|map| map.map(character).is_none_or(|glyph| glyph == 0))
            {
                return None;
            }
            let id = self.install(selected, database)?;
            self.origins.entry(id).or_insert_with(|| spec.clone());
            Some(id)
        }

        fn install(&mut self, selected: SelectedFont, database: &mut Arc<Database>) -> Option<ID> {
            let key = (selected.font.blob.id(), selected.font.index);
            if let Some(id) = self.ids.get(&key) {
                return Some(*id);
            }
            let bytes = selected.font.blob.as_ref();
            if bytes.len() > MAX_FONT_BYTES.saturating_sub(self.loaded_bytes)
                || self.ids.len() >= 256
            {
                return None;
            }
            // fontdb allocates its collection ID vector from the TTC face count.
            // Cap it before handing over even locally installed font bytes.
            if !collection_fits(bytes, self.ids.len()) {
                return None;
            }
            let ids = Arc::make_mut(database)
                .load_font_source(Source::Binary(Arc::new(selected.font.blob.clone())));
            self.loaded_bytes += bytes.len();
            let mut chosen = None;
            for id in ids {
                let face = database.face(id)?;
                self.ids.insert((key.0, face.index), id);
                if face.index == key.1 {
                    chosen = Some(id);
                }
            }
            chosen
        }
    }

    fn collection_fits(bytes: &[u8], installed: usize) -> bool {
        let count = if bytes.starts_with(b"ttcf") {
            let Some(header) = bytes.get(8..12) else {
                return false;
            };
            u32::from_be_bytes(header.try_into().expect("four-byte TTC count")) as usize
        } else {
            1
        };
        count > 0 && count <= 256usize.saturating_sub(installed)
    }

    #[cfg(test)]
    mod tests {
        use super::collection_fits;

        #[test]
        fn named_family_serialization_preserves_commas_quotes_and_backslashes() {
            let families = [
                svgtypes::FontFamily::Named("Alias, One".into()),
                svgtypes::FontFamily::Named("Alias\"\\Two".into()),
                svgtypes::FontFamily::SansSerif,
            ];
            let serialized = super::family_list(&families);
            use crate::engine::css::font_family::{Family, parse};
            assert_eq!(
                parse(&serialized).unwrap(),
                [
                    Family::Named("Alias, One".into()),
                    Family::Named("Alias\"\\Two".into()),
                    Family::Generic("sans-serif".into()),
                ]
            );
        }

        #[test]
        fn collection_face_budget_includes_previously_loaded_faces() {
            let mut collection = *b"ttcf\0\x01\0\0\0\0\0\x02";
            assert!(collection_fits(&collection, 254));
            assert!(!collection_fits(&collection, 255));
            collection[8..12].copy_from_slice(&256u32.to_be_bytes());
            assert!(collection_fits(&collection, 0));
            assert!(!collection_fits(&collection, 1));
            collection[8..12].copy_from_slice(&u32::MAX.to_be_bytes());
            assert!(!collection_fits(&collection, 0));
        }

        #[test]
        fn empty_truncated_and_exhausted_collections_fail_closed() {
            assert!(!collection_fits(b"ttcf", 0));
            assert!(!collection_fits(b"ttcf\0\x01\0\0\0\0\0\0", 0));
            assert!(collection_fits(b"\0\x01\0\0", 255));
            assert!(!collection_fits(b"\0\x01\0\0", 256));
            assert!(!collection_fits(b"\0\x01\0\0", usize::MAX));
        }
    }
}
