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
        for node in Node::shadow_including_descendants(&self.dom.document) {
            // Hydrate newly connected chains through the same computed-style API.
            let Some(style) = styles.computed_style_for_node(&node) else {
                continue;
            };
            if style.display == Display::None || !style.visibility {
                continue;
            }
            for url in [&style.background_image, &style.mask_image]
                .into_iter()
                .flatten()
            {
                if discovered_style_images < MAX_STYLE_IMAGES && known_images.insert(url.clone()) {
                    self.resources
                        .push(PageResource::Image { url: url.clone() });
                    discovered_style_images += 1;
                }
            }
        }
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
