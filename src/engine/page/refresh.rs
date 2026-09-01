use super::*;
use crate::engine::css::Display;
use crate::engine::dom::Node;
use crate::engine::font::discover_font_faces;
use crate::engine::invalidation::RenderInvalidation;
use std::collections::HashSet;

const MAX_INLINE_SVG_DIAGNOSTICS: usize = 8;
const MAX_INLINE_SVG_DIAGNOSTIC_BYTES: usize = 512;

pub(super) fn parse_immediate_refresh_target(content: &str) -> Option<&str> {
    let (delay, directive) = content.split_once(';')?;
    if delay.trim().parse::<f64>().ok()? > 0.0 {
        return None;
    }
    let (name, target) = directive.trim().split_once('=')?;
    if !name.trim().eq_ignore_ascii_case("url") {
        return None;
    }
    let target = target
        .trim()
        .trim_matches(|character| matches!(character, '\'' | '"'))
        .trim();
    (!target.is_empty()).then_some(target)
}

impl Page {
    pub fn style(&self, viewport_width: f32) -> StyleSet {
        self.style_for_viewport(viewport_width, viewport_width)
    }

    pub fn style_for_viewport(&self, viewport_width: f32, viewport_height: f32) -> StyleSet {
        StyleSet::from_sources_for_media_environment(
            &self.dom,
            &self.base_url,
            &self.stylesheet_sources,
            self.media_environment
                .with_viewport(viewport_width, viewport_height),
        )
    }

    pub fn cached_style(&self, viewport_width: f32) -> Option<&StyleSet> {
        self.cached_style_for_viewport(viewport_width, viewport_width)
    }

    pub fn cached_style_for_viewport(
        &self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> Option<&StyleSet> {
        self.cached_styles
            .as_ref()
            .filter(|(width, height, _)| {
                (*width - viewport_width).abs() < 0.5 && (*height - viewport_height).abs() < 0.5
            })
            .map(|(_, _, styles)| styles)
    }

    pub fn resource_blocks_first_paint(&self, resource: &PageResource) -> bool {
        match resource {
            PageResource::Script {
                url,
                kind,
                fetch_options,
            } => self.scripts.iter().any(|script| {
                script.source_url.as_str() == url
                    && script.kind == *kind
                    && script.fetch_options == *fetch_options
                    && script.blocks_first_paint
            }),
            PageResource::Stylesheet { .. } => true,
            PageResource::Image { .. } | PageResource::Media { .. } | PageResource::Font { .. } => {
                false
            }
        }
    }

    pub fn refresh_resources(&mut self, viewport_width: f32) -> StyleRefreshStats {
        self.refresh_resources_for_viewport(viewport_width, viewport_width)
    }

    pub fn refresh_resources_for_viewport(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
    ) -> StyleRefreshStats {
        self.refresh_resources_after_invalidation_for_viewport(
            viewport_width,
            viewport_height,
            &RenderInvalidation::full(self.dom.document.id()),
        )
    }

    pub fn refresh_resources_after_invalidation(
        &mut self,
        viewport_width: f32,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        self.refresh_resources_after_invalidation_for_viewport(
            viewport_width,
            viewport_width,
            invalidation,
        )
    }

    pub fn refresh_resources_after_invalidation_for_viewport(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.media_environment = self
            .media_environment
            .with_viewport(viewport_width, viewport_height);
        let (resources, _) = discover_resources(&self.dom, &self.base_url, self.media_environment);
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
        for root in Node::shadow_including_descendants(&self.dom.document) {
            for stylesheet in root.adopted_stylesheets() {
                available_faces.extend(discover_font_faces(
                    &stylesheet.source,
                    &stylesheet.base_url,
                ));
            }
        }
        let (mut styles, style_stats) =
            self.refresh_style_cache(viewport_width, viewport_height, invalidation);
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
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
        for node in Node::shadow_including_descendants(&self.dom.document) {
            // Dynamic DOM work can connect a previously detached subtree at a rendering
            // checkpoint. Hydrate any missing ancestor chain defensively instead of letting a
            // stale incremental root turn untrusted page input into a browser-process panic.
            let Some(style) = styles.computed_style_for_node(&node) else {
                continue;
            };
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
        self.cached_styles = Some((viewport_width, viewport_height, styles));
        self.refresh_inline_svgs();
        style_stats
    }

    /// Refreshes only the style cache needed by a synchronous CSSOM View layout flush.
    pub(crate) fn refresh_layout_styles_after_invalidation_for_viewport(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.media_environment = self
            .media_environment
            .with_viewport(viewport_width, viewport_height);
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
        let (styles, stats) =
            self.refresh_style_cache(viewport_width, viewport_height, invalidation);
        self.cached_styles = Some((viewport_width, viewport_height, styles));
        stats
    }

    fn refresh_style_cache(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
    ) -> (StyleSet, StyleRefreshStats) {
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
        let mut invalidation_roots = invalidation
            .roots
            .iter()
            .filter_map(|root| self.dom.find_node(*root))
            .collect::<Vec<_>>();
        if invalidation_roots.is_empty() {
            invalidation_roots.push(self.dom.document.clone());
        }
        let invalidated_nodes = invalidation_roots
            .iter()
            .flat_map(Node::shadow_including_descendants)
            .map(|node| node.id())
            .collect::<HashSet<_>>()
            .len();
        let cached = self.cached_styles.take();
        match cached {
            Some((cached_width, cached_height, mut styles))
                if !invalidation.rebuild_style_rules
                    && (cached_width - viewport_width).abs() < 0.5
                    && (cached_height - viewport_height).abs() < 0.5 =>
            {
                let stats = if invalidation.impact.affects_style() {
                    styles.refresh_subtrees(
                        &self.dom.document,
                        &invalidation_roots,
                        &invalidation.removed_nodes,
                    )
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
                let styles = StyleSet::from_sources_for_media_environment(
                    &self.dom,
                    &self.base_url,
                    &self.stylesheet_sources,
                    self.media_environment
                        .with_viewport(viewport_width, viewport_height),
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
        }
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
        let svgs = Node::shadow_including_descendants(&self.dom.document)
            .filter(|node| node.tag_name() == Some("svg"))
            .take(MAX_INLINE_SVGS)
            .collect::<Vec<_>>();
        let active_ids = svgs.iter().map(|svg| svg.id()).collect::<HashSet<_>>();
        let active_keys = svgs.iter().map(inline_svg_key).collect::<HashSet<_>>();
        self.inline_svg_versions
            .retain(|node, _| active_ids.contains(node));
        self.images
            .retain(|key, _| !key.starts_with("inline-svg:") || active_keys.contains(key));

        for svg in svgs {
            let key = inline_svg_key(&svg);
            let version = svg.subtree_mutation_version();
            let changed = self.inline_svg_versions.get(&svg.id()).copied() != Some(version);
            if !changed {
                continue;
            }
            self.inline_svg_versions.insert(svg.id(), version);
            match decode_inline_svg(&svg) {
                Ok(image) => {
                    let _ = self.install_decoded_image(key, image);
                }
                Err(error) => {
                    self.images.remove(&key);
                    if self
                        .diagnostics
                        .iter()
                        .filter(|message| message.starts_with("inline SVG "))
                        .count()
                        < MAX_INLINE_SVG_DIAGNOSTICS
                    {
                        let message = format!("inline SVG {:032x}: {error}", svg.id().to_wire());
                        self.diagnostics.push(
                            bounded_utf8_prefix(&message, MAX_INLINE_SVG_DIAGNOSTIC_BYTES)
                                .0
                                .to_string(),
                        );
                    }
                }
            }
        }
    }
}
