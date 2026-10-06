//! CSS font loading identity is the original face source, not a fallback URL.
//! Ignore unused and disconnected rules; retain one loading period through
//! transport/decode fallback until a current candidate succeeds or exhausts.
use super::super::*;
use std::collections::HashSet;

impl Page {
    pub(crate) fn pending_css_font_sources(
        &self,
        completed: &HashSet<PageResource>,
    ) -> HashSet<crate::engine::font::loading_identity::FontLoadIdentity> {
        let candidates = self.resources.iter().filter(|resource| {
            matches!(resource, PageResource::Font { .. }) && !completed.contains(*resource)
        });
        if candidates.clone().next().is_none() {
            return HashSet::new();
        }
        // Parse ownership rules once for all pending requests, not once per
        // font on every renderer scheduling checkpoint.
        let styles = StyleSet::from_sources_for_media_environment(
            &self.dom,
            &self.base_url,
            &self.stylesheet_sources,
            self.media_environment,
        );
        let faces = styles.document_font_faces();
        candidates
            .filter_map(|resource| {
                let face = faces.iter().find(|face| {
                    super::fallback::matches_face(resource, std::slice::from_ref(face))
                })?;
                if self
                    .fonts
                    .iter()
                    .any(|font| face.matches_loaded_css_font(font))
                {
                    return None;
                }
                Some(face.loading_identity())
            })
            .collect()
    }
}

#[cfg(test)]
mod tests;
