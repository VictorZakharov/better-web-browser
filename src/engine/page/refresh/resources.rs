//! Post-cascade resource work shared by ordinary and failed retained proofs.
use super::*;

impl Page {
    pub(super) fn finish_refreshed_resources(
        &mut self,
        mut styles: StyleSet,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        let started = self.resource_profile.start();
        let mut known_images = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Image { url } => Some(url.clone()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut discovered_style_images = 0_usize;
        let mut discover = |style: &crate::engine::css::ComputedStyle,
                            resources: &mut Vec<PageResource>| {
            if style.display == Display::None || !style.visibility {
                return;
            }
            for url in style.background_image.urls().chain(style.mask_image.urls()) {
                if discovered_style_images < MAX_STYLE_IMAGES && known_images.insert(url.clone()) {
                    resources.push(PageResource::Image { url: url.clone() });
                    discovered_style_images += 1;
                }
            }
        };
        // Unchanged styles name only images found by an earlier pass, unless that pass stopped
        // at the per-pass limit. Styles of removed elements are gone from the set.
        match styles.take_style_log() {
            Some(changed) if !self.style_images_pending => {
                for id in changed {
                    if let Some(style) = styles.styles.get(&id) {
                        discover(style, &mut self.resources);
                    }
                }
            }
            _ => {
                for node in Node::shadow_including_descendants(&self.dom.document) {
                    // Hydrate newly connected chains through the same computed-style API.
                    if let Some(style) = styles.computed_style_for_node(&node) {
                        discover(style, &mut self.resources);
                    }
                }
            }
        }
        self.style_images_pending = discovered_style_images >= MAX_STYLE_IMAGES;
        self.resource_profile.finish(Phase::StyleImages, started);
        let started = self.resource_profile.start();
        self.install_embedded_images();
        self.resource_profile.finish(Phase::EmbeddedImages, started);
        let started = self.resource_profile.start();
        self.request_visible_fonts(&mut styles);
        self.resource_profile.finish(Phase::Fonts, started);
        self.cached_styles = Some((viewport_width.max(1.0), viewport_height.max(1.0), styles));
        let started = self.resource_profile.start();
        self.refresh_inline_svgs();
        self.resource_profile.finish(Phase::Svg, started);
    }
}

#[cfg(test)]
mod tests;
