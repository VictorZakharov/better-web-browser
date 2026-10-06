//! Remote CSS font alternatives keep the original face identity while each
//! candidate passes normal Fetch/CORS/CSP and font-byte validation independently.
use super::super::*;

impl Page {
    pub(crate) fn is_current_font_resource(&self, resource: &PageResource) -> bool {
        let PageResource::Font {
            url,
            source_url,
            fallback_urls,
            family,
            weight,
            italic,
            unicode_range,
            font_feature_settings,
        } = resource
        else {
            return true;
        };
        let styles = StyleSet::from_sources_for_media_environment(
            &self.dom,
            &self.base_url,
            &self.stylesheet_sources,
            self.media_environment,
        );
        styles.document_font_faces().iter().any(|face| {
            let urls = std::iter::once(&face.url)
                .chain(&face.fallback_urls)
                .collect::<Vec<_>>();
            let candidate = urls.iter().enumerate().any(|(index, candidate)| {
                candidate.as_str() == url
                    && urls[index + 1..].iter().copied().eq(fallback_urls.iter())
            });
            candidate
                && face.url == *source_url
                && face.family == *family
                && face.italic == *italic
                && face.registered_weight(*weight) == *weight
                && face.unicode_range == *unicode_range
                && face.features.css_text() == *font_feature_settings
        })
    }

    /// Called only after an admitted candidate failed. Never retry a removed or
    /// changed rule; deduplicate queued alternatives across completion callbacks.
    pub(crate) fn retry_font_resource(&mut self, resource: &PageResource) -> bool {
        if !self.is_current_font_resource(resource) {
            return false;
        }
        let mut next = resource.clone();
        let PageResource::Font {
            url, fallback_urls, ..
        } = &mut next
        else {
            return false;
        };
        if fallback_urls.is_empty() {
            return false;
        }
        *url = fallback_urls.remove(0);
        if !self.resources.contains(&next) {
            self.resources.push(next);
        }
        true
    }
}

#[cfg(test)]
mod tests;
