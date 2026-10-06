//! Preserve the browser response's pixel-readback authority beside decoded images.
use super::*;

impl Page {
    /// This must use the final Fetch response, never an element's mutable crossOrigin value.
    pub(crate) fn set_image_origin_clean(&mut self, url: &str, clean: bool) {
        if self.images.contains_key(url) {
            self.image_origin_clean.insert(url.to_owned(), clean);
        }
    }

    pub(crate) fn synchronize_script_images(&self, runtime: &mut ScriptRuntime) {
        runtime.set_document_images(&self.images, &self.image_origin_clean);
    }

    pub(crate) fn image_origin_policy(&self) -> &HashMap<String, bool> {
        &self.image_origin_clean
    }

    pub(crate) fn selected_image_source(
        node: &NodeRef,
        base: &str,
        environment: MediaEnvironment,
    ) -> Option<String> {
        // Canvas sources obey HTML img attributes, not the renderer's legacy
        // data-src/data-lazy-src painting conveniences or SVG href fallback.
        resources::resolve_html_image_url(node, base, environment)
    }
}
