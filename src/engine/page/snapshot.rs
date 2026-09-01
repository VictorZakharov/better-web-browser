use super::*;

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
            images: self.images.clone(),
            inline_svg_versions: HashMap::new(),
            fonts: Vec::new(),
            diagnostics: Vec::new(),
            media_environment: self.media_environment,
            layout_viewport: self.layout_viewport,
        }
    }
}
