//! CSS Font Loading's pending-on-the-environment gate. Font completion must
//! not promise stable metrics before parsing, stylesheets and layout settle.
//! https://drafts.csswg.org/css-font-loading/#fontfaceset-pending-on-the-environment

use super::*;

pub(in crate::engine::script) struct FontEnvironment {
    pub(in crate::engine::script) stylesheet_pending: bool,
    pub(in crate::engine::script) pending_css_fonts:
        HashSet<crate::engine::font::loading_identity::FontLoadIdentity>,
    pub(in crate::engine::script) requested_css_fonts:
        HashSet<crate::engine::font::loading_identity::FontLoadIdentity>,
    revision: u64,
    published_revision: u64,
    pub(in crate::engine::script) notified_pending: Option<bool>,
    pub(in crate::engine::script) observing: bool,
}

impl Default for FontEnvironment {
    fn default() -> Self {
        Self {
            stylesheet_pending: false,
            pending_css_fonts: HashSet::new(),
            requested_css_fonts: HashSet::new(),
            revision: 0,
            published_revision: 0,
            notified_pending: None,
            observing: true,
        }
    }
}

impl HostState {
    pub(in crate::engine::script) fn connected_font_faces(
        &mut self,
    ) -> Vec<crate::engine::font::WebFontFace> {
        let mut seen = HashSet::new();
        self.document_font_faces()
            .into_iter()
            .filter(|face| {
                seen.insert((
                    face.family.clone(),
                    face.weight,
                    face.italic,
                    face.url.clone(),
                    face.unicode_range.clone(),
                    face.features.clone(),
                    face.fallback_urls.clone(),
                ))
            })
            .take(64)
            .collect()
    }

    pub(in crate::engine::script) fn font_environment_pending(&self) -> bool {
        !self.document_load.complete()
            || self.font_environment.stylesheet_pending
            || !self.font_environment.pending_css_fonts.is_empty()
            || self.layout_flush.is_some()
                && (!self.pending_font_actions.is_empty()
                    || !self.layout_geometry_initialized
                    || self.layout_geometry_version != self.document.subtree_mutation_version()
                    || self.font_environment.revision != self.font_environment.published_revision)
    }

    pub(in crate::engine::script) fn font_environment_needs_notification(&self) -> bool {
        self.font_environment.notified_pending.is_none()
            || self.font_environment.observing
                && self.font_environment.notified_pending != Some(self.font_environment_pending())
    }

    pub(in crate::engine::script) fn invalidate_font_layout(&mut self) {
        self.font_environment.revision = self.font_environment.revision.wrapping_add(1);
        self.layout_geometry_initialized = false;
    }

    pub(in crate::engine::script) fn acknowledge_font_layout(&mut self) {
        self.font_environment.published_revision = self.font_environment.revision;
    }
}
