use super::*;
use crate::engine::css::Display;
use crate::engine::dom::Node;
use crate::engine::font::discover_font_faces;
use crate::engine::invalidation::RenderInvalidation;
use std::collections::HashSet;

impl Page {
    pub fn resource_blocks_first_paint(&self, resource: &PageResource) -> bool {
        match resource {
            PageResource::Script { url, .. } => self
                .scripts
                .iter()
                .any(|script| script.source_url.as_str() == url && script.blocks_first_paint),
            PageResource::Stylesheet { .. } => true,
            PageResource::Image { .. } | PageResource::Font { .. } => false,
        }
    }

    pub fn refresh_resources(&mut self, viewport_width: f32) -> StyleRefreshStats {
        self.refresh_resources_after_invalidation(
            viewport_width,
            &RenderInvalidation::full(self.dom.document.id()),
        )
    }

    pub fn refresh_resources_after_invalidation(
        &mut self,
        viewport_width: f32,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.responsive_viewport_width = viewport_width.max(1.0);
        let (resources, _) =
            discover_resources(&self.dom, &self.base_url, self.responsive_viewport_width);
        for resource in resources {
            if !matches!(resource, PageResource::Script { .. })
                && !self.resources.contains(&resource)
            {
                self.resources.push(resource);
            }
        }
        let mut available_faces = Vec::new();
        for (source_url, css) in &self.stylesheet_sources {
            available_faces.extend(discover_font_faces(css, source_url));
        }
        let viewport_width = viewport_width.max(1.0);
        let invalidated_nodes = invalidation
            .root
            .and_then(|root| self.dom.find_node(root))
            .map(|root| Node::descendants(&root).count())
            .unwrap_or_else(|| Node::descendants(&self.dom.document).count());
        let cached = self.cached_styles.take();
        let (styles, style_stats) = match cached {
            Some((cached_width, mut styles))
                if !invalidation.rebuild_style_rules
                    && (cached_width - viewport_width).abs() < 0.5 =>
            {
                let stats = if invalidation.impact.affects_style() {
                    let root = invalidation
                        .root
                        .and_then(|root| self.dom.find_node(root))
                        .unwrap_or_else(|| self.dom.document.clone());
                    styles.refresh_subtree(&self.dom.document, &root, &invalidation.removed_nodes)
                } else {
                    StyleRefreshStats {
                        invalidated_nodes,
                        total_styles: styles.styles.len(),
                        ..StyleRefreshStats::default()
                    }
                };
                (styles, stats)
            }
            _ => {
                let styles = StyleSet::from_sources(
                    &self.dom,
                    &self.base_url,
                    &self.stylesheet_sources,
                    viewport_width,
                );
                let count = styles.styles.len();
                (
                    styles,
                    StyleRefreshStats {
                        invalidated_nodes,
                        total_styles: count,
                        recomputed_styles: count,
                        changed_styles: count,
                        layout_changed: true,
                        full_rebuild: true,
                        ..StyleRefreshStats::default()
                    },
                )
            }
        };
        let mut known_images = self
            .resources
            .iter()
            .filter_map(|resource| match resource {
                PageResource::Image { url } => Some(url.clone()),
                _ => None,
            })
            .collect::<HashSet<_>>();
        let mut requested_faces = Vec::<(String, u16, bool)>::new();
        let mut discovered_style_images = 0_usize;
        for node in Node::descendants(&self.dom.document) {
            let style = styles.get(&node);
            if style.display == Display::None || !style.visibility {
                continue;
            }
            if discovered_style_images < MAX_STYLE_IMAGES
                && let Some(url) = style.background_image.as_ref()
                && known_images.insert(url.clone())
            {
                self.resources
                    .push(PageResource::Image { url: url.clone() });
                discovered_style_images += 1;
            }
            if discovered_style_images < MAX_STYLE_IMAGES
                && let Some(url) = style.mask_image.as_ref()
                && known_images.insert(url.clone())
            {
                self.resources
                    .push(PageResource::Image { url: url.clone() });
                discovered_style_images += 1;
            }
            let family = style
                .font_family
                .split(',')
                .next()
                .unwrap_or("")
                .trim()
                .trim_matches(['\'', '"'])
                .to_ascii_lowercase();
            if !family.is_empty()
                && !requested_faces.iter().any(|(requested, weight, italic)| {
                    requested == &family && *weight == style.font_weight && *italic == style.italic
                })
            {
                requested_faces.push((family, style.font_weight, style.italic));
            }
        }
        self.install_embedded_images();
        self.add_requested_fonts(available_faces, requested_faces);
        self.cached_styles = Some((viewport_width, styles));
        self.refresh_inline_svgs();
        style_stats
    }

    fn add_requested_fonts(
        &mut self,
        available_faces: Vec<WebFontFace>,
        requested_faces: Vec<(String, u16, bool)>,
    ) {
        let mut selected_faces = Vec::<WebFontFace>::new();
        for (family, weight, italic) in requested_faces {
            let Some(face) = available_faces
                .iter()
                .filter(|face| face.family.eq_ignore_ascii_case(&family))
                .min_by_key(|face| {
                    (
                        u8::from(face.italic != italic),
                        face.weight.abs_diff(weight),
                    )
                })
            else {
                continue;
            };
            if !selected_faces.contains(face) {
                selected_faces.push(face.clone());
            }
        }
        for face in selected_faces.into_iter().take(MAX_WEB_FONTS) {
            let resource = PageResource::Font {
                url: face.url,
                family: face.family,
                weight: face.weight,
                italic: face.italic,
            };
            if !self.resources.contains(&resource) {
                self.resources.push(resource);
            }
        }
    }

    fn refresh_inline_svgs(&mut self) {
        for svg in self.dom.elements_named("svg").take(MAX_INLINE_SVGS) {
            let key = inline_svg_key(&svg);
            if !self.images.contains_key(&key)
                && let Ok(image) = decode_inline_svg(&svg)
            {
                self.images.insert(key, image);
            }
        }
    }
}
