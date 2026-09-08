use super::*;

fn layout_image_metadata(images: &HashMap<String, DecodedImage>) -> HashMap<String, DecodedImage> {
    images
        .iter()
        .map(|(key, image)| {
            (
                key.clone(),
                DecodedImage {
                    width: image.width,
                    height: image.height,
                    // CSS layout needs intrinsic dimensions and the stable resource key, not a
                    // second copy of immutable BGRA pixels. Painting always uses the owning Page.
                    bgra: Arc::from([]),
                },
            )
        })
        .collect()
}

impl Page {
    /// Creates the narrow page snapshot used by synchronous CSSOM View layout flushes.
    ///
    /// The DOM clone retains the same node graph and mutation identity. Resource and stylesheet
    /// state is copied at rendering checkpoints, while derived style/layout caches are rebuilt so
    /// script geometry observes mutations made in the current task.
    pub(crate) fn layout_snapshot(&self) -> Self {
        Self {
            dom: self.dom.clone(),
            title: self.title.clone(),
            source_url: self.source_url.clone(),
            character_set: self.character_set.clone(),
            base_url: self.base_url.clone(),
            resources: Vec::new(),
            scripts: Vec::new(),
            external_stylesheets: self.external_stylesheets.clone(),
            stylesheet_sources: self.stylesheet_sources.clone(),
            cached_styles: None,
            images: layout_image_metadata(&self.images),
            inline_svg_versions: HashMap::new(),
            fonts: Vec::new(),
            diagnostics: Vec::new(),
            media_environment: self.media_environment,
            layout_viewport: self.layout_viewport,
        }
    }

    /// Copies source and resource state while retaining the snapshot's independent style cache.
    pub(crate) fn synchronize_layout_snapshot(&mut self, source: &Self) {
        let style_sources_changed = self.source_url != source.source_url
            || self.stylesheet_sources != source.stylesheet_sources
            || self.media_environment != source.media_environment;
        self.dom = source.dom.clone();
        self.title = source.title.clone();
        self.source_url = source.source_url.clone();
        self.character_set = source.character_set.clone();
        self.base_url = source.base_url.clone();
        self.external_stylesheets = source.external_stylesheets.clone();
        self.stylesheet_sources = source.stylesheet_sources.clone();
        self.images = layout_image_metadata(&source.images);
        self.media_environment = source.media_environment;
        self.layout_viewport = source.layout_viewport;
        if style_sources_changed {
            self.cached_styles = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedMeasurer;
    impl crate::engine::TextMeasurer for FixedMeasurer {
        fn measure(&mut self, text: &str, font: &crate::engine::FontSpec) -> (f32, f32) {
            (text.chars().count() as f32 * font.size * 0.5, font.size)
        }
    }

    #[test]
    fn sparse_layout_snapshots_preserve_visible_geometry_and_hidden_subtrees() {
        for markup in [
            "<body><div style='display:none'><section><span>hidden</span></section></div><p>visible</p>",
            "<html style='display:none'><body><p>hidden document</p>",
            "<body style='display:none'><main><p>hidden body</p></main>",
            "<body><button>Play<span style='display:none'><svg width=100 height=100></svg><i>hidden label</i></span></button>",
            "<body><div style='display:contents'><p>visible contents</p></div>",
        ] {
            let page = Page::parse(markup, "https://example.com/");
            let mut snapshot = page.layout_snapshot();
            snapshot.refresh_layout_styles_after_invalidation_for_viewport(
                800.0,
                600.0,
                &crate::engine::invalidation::RenderInvalidation::full(page.dom.document.id()),
            );
            let full = crate::engine::layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
            let sparse = crate::engine::layout_page(&snapshot, 800.0, 600.0, &mut FixedMeasurer);
            assert_eq!(sparse.node_bounds, full.node_bounds, "{markup}");
            assert_eq!(sparse.content_height, full.content_height, "{markup}");
        }
    }

    #[test]
    fn layout_snapshots_retain_image_dimensions_without_copying_pixels() {
        let mut page = Page::parse("<img src='hero.png'>", "https://example.com/");
        page.images.insert(
            "https://example.com/hero.png".into(),
            DecodedImage {
                width: 3,
                height: 2,
                bgra: vec![7; 24].into(),
            },
        );

        let mut snapshot = page.layout_snapshot();
        let image = &snapshot.images["https://example.com/hero.png"];
        assert_eq!((image.width, image.height), (3, 2));
        assert!(image.bgra.is_empty());
        assert_eq!(page.images["https://example.com/hero.png"].bgra.len(), 24);

        page.images.insert(
            "https://example.com/hero.png".into(),
            DecodedImage {
                width: 5,
                height: 4,
                bgra: vec![9; 80].into(),
            },
        );
        snapshot.synchronize_layout_snapshot(&page);
        let image = &snapshot.images["https://example.com/hero.png"];
        assert_eq!((image.width, image.height), (5, 4));
        assert!(image.bgra.is_empty());
    }
}
