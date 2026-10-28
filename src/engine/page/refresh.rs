use super::*;
pub(super) mod profile;
mod resources;
mod unchanged;
use crate::engine::css::Display;
use crate::engine::dom::Node;
use crate::engine::invalidation::{MAX_DEFERRED_INVALIDATION_ROOTS, RenderInvalidation};
use profile::Phase;
use std::collections::HashSet;

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
    pub(crate) fn scrollbar_thickness(&self) -> f32 {
        let scale = self.media_environment.resolution_dppx.max(0.1);
        (15.0 * scale).ceil() / scale
    }
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

    /// Computes and caches styles, for a caller that inspects them before laying out the
    /// same viewport (which then reuses them).
    pub(crate) fn cache_styles_for_viewport(&mut self, width: f32, height: f32) -> &StyleSet {
        let styles = self.style_for_viewport(width, height);
        self.store_styles(width, height, styles);
        &self.cached_styles.as_ref().unwrap().2
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
            .map(|(_, _, styles)| &**styles)
    }

    pub fn resource_blocks_first_paint(&self, resource: &PageResource) -> bool {
        match resource {
            PageResource::Script {
                url,
                kind,
                fetch_options,
                ..
            } => self.scripts.iter().any(|script| {
                script.source_url.as_str() == url
                    && script.kind == *kind
                    && script.fetch_options == *fetch_options
                    && script.blocks_first_paint
            }),
            PageResource::Stylesheet { .. } => true,
            PageResource::OriginHint { .. }
            | PageResource::Prefetch { .. }
            | PageResource::Preload { .. }
            | PageResource::Image { .. }
            | PageResource::Media { .. }
            | PageResource::Font { .. } => false,
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
        self.resource_profile.reset();
        self.discover_refreshed_resources(viewport_width, viewport_height);
        let started = self.resource_profile.start();
        let (styles, style_stats) =
            self.refresh_style_cache(viewport_width, viewport_height, invalidation, false);
        self.resource_profile.finish(Phase::Cascade, started);
        self.finish_refreshed_resources(styles, viewport_width, viewport_height);
        style_stats
    }

    /// Discovers resources as a refresh does but leaves the cascade to a later refresh, which
    /// rebuilds every style through a deferred whole-document invalidation. Style-dependent discovery
    /// (background images and fonts) waits for that refresh as well.
    pub fn refresh_resources_deferring_styles(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
    ) {
        self.resource_profile.reset();
        self.discover_refreshed_resources(viewport_width, viewport_height);
        if let Some(styles) = self.cached_styles.take() {
            self.previous_styles = Some(styles);
        }
        let root = self.dom.document.id();
        self.deferred_style_invalidation = Some(RenderInvalidation::full(root));
    }

    /// Discovers resources as a refresh does but keeps `invalidation` for the next style
    /// refresh, whichever path performs it, so that a rendering checkpoint can skip its frame
    /// without making the next frame restyle the whole document.
    pub fn defer_style_invalidation(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
    ) {
        self.resource_profile.reset();
        self.discover_refreshed_resources(viewport_width, viewport_height);
        let root = self.dom.document.id();
        self.deferred_style_invalidation
            .get_or_insert_with(RenderInvalidation::default)
            .merge_with_root_limit(invalidation.clone(), root, MAX_DEFERRED_INVALIDATION_ROOTS);
    }

    pub fn has_deferred_style_invalidation(&self) -> bool {
        self.deferred_style_invalidation.is_some()
    }

    fn discover_refreshed_resources(&mut self, viewport_width: f32, viewport_height: f32) {
        let started = self.resource_profile.start();
        self.base_url = self.document_base_url();
        self.media_environment = self
            .media_environment
            .with_viewport(viewport_width, viewport_height);
        // Script preparation belongs to the parser/runtime, not a rendering
        // checkpoint. Do not clone inline source only to discard it below.
        // Media elements' resources come from `refresh_media_sources` below.
        let resources = self.discover_link_and_image_resources();
        let known = self.resources.iter().collect::<HashSet<_>>();
        let added = resources
            .into_iter()
            .filter(|resource| {
                !matches!(
                    resource,
                    PageResource::Script { .. } | PageResource::Media { .. }
                ) && !known.contains(resource)
            })
            .collect::<Vec<_>>();
        self.resources.extend(added);
        self.resource_profile.finish(Phase::Discovery, started);
        let started = self.resource_profile.start();
        self.refresh_media_sources();
        self.resource_profile.finish(Phase::Media, started);
        let started = self.resource_profile.start();
        self.discover_stylesheet_dependencies();
        self.resource_profile.finish(Phase::Stylesheets, started);
    }

    /// Refreshes only the style cache needed by a synchronous CSSOM View layout flush.
    pub(crate) fn refresh_layout_styles_after_invalidation_for_viewport(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
    ) -> StyleRefreshStats {
        self.base_url = self.document_base_url();
        self.media_environment = self
            .media_environment
            .with_viewport(viewport_width, viewport_height);
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
        let seeded = self.adopt_style_seed();
        let (styles, mut stats) =
            self.refresh_style_cache(viewport_width, viewport_height, invalidation, true);
        self.store_styles(viewport_width, viewport_height, styles);
        // Geometry published before the seed may predate it; lay out once, as a full style
        // build would.
        stats.layout_changed |= seeded;
        stats
    }

    /// Stores refreshed styles with the document version they describe.
    pub(super) fn store_styles(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        styles: StyleSet,
    ) {
        self.styled_version = Some(self.dom.document.subtree_mutation_version());
        self.cached_styles = Some((
            viewport_width.max(1.0),
            viewport_height.max(1.0),
            std::rc::Rc::new(styles),
        ));
    }

    fn refresh_style_cache(
        &mut self,
        viewport_width: f32,
        viewport_height: f32,
        invalidation: &RenderInvalidation,
        layout_only: bool,
    ) -> (StyleSet, StyleRefreshStats) {
        let viewport_width = viewport_width.max(1.0);
        let viewport_height = viewport_height.max(1.0);
        // A layout-only flush computes part of each style, so deferred work stays pending for
        // the next complete refresh as well.
        let deferred = if layout_only {
            self.deferred_style_invalidation.clone()
        } else {
            self.deferred_style_invalidation.take()
        };
        let merged;
        let invalidation = match deferred {
            Some(mut deferred) => {
                deferred.merge_with_root_limit(
                    invalidation.clone(),
                    self.dom.document.id(),
                    MAX_DEFERRED_INVALIDATION_ROOTS,
                );
                merged = deferred;
                &merged
            }
            None => invalidation,
        };
        let mut invalidation_roots = self.dom.find_nodes_in_order(&invalidation.roots);
        if invalidation_roots.is_empty() && invalidation.attribute_changes.is_empty() {
            invalidation_roots.push(self.dom.document.clone());
        }
        // Roots inside another root add nothing; the rest cover disjoint subtrees.
        let root_ids = invalidation_roots
            .iter()
            .map(|root| root.id())
            .collect::<HashSet<_>>();
        let mut counted = HashSet::new();
        let invalidated_nodes = invalidation_roots
            .iter()
            .filter(|root| {
                counted.insert(root.id())
                    && !std::iter::successors(root.shadow_including_parent(), |node| {
                        node.shadow_including_parent()
                    })
                    .any(|ancestor| root_ids.contains(&ancestor.id()))
            })
            .map(|root| Node::shadow_including_descendants(root).count())
            .sum::<usize>();
        let cached = self.cached_styles.take();
        let stale = cached.is_none() && self.previous_styles.is_some();
        let cached = cached.or_else(|| self.previous_styles.take());
        // A snapshot's seed holds only a weak reference, so this unwraps without copying.
        let cached = cached.map(|(width, height, styles)| {
            let styles = std::rc::Rc::try_unwrap(styles).unwrap_or_else(|shared| shared.fork());
            (width, height, styles)
        });
        match cached {
            Some((cached_width, cached_height, mut styles))
                if !stale
                    && !invalidation.rebuild_style_rules
                    && (cached_width - viewport_width).abs() < 0.5
                    && (cached_height - viewport_height).abs() < 0.5 =>
            {
                let stats = if invalidation.impact.affects_style() {
                    let mut element_roots = Vec::new();
                    let fallbacks = styles.resolve_attribute_changes(
                        &self.dom,
                        &invalidation.attribute_changes,
                        &mut invalidation_roots,
                        &mut element_roots,
                    );
                    styles.resolve_child_list_changes(
                        &self.dom,
                        &invalidation.child_list_roots,
                        &invalidation.removed_nodes,
                        &mut invalidation_roots,
                        &mut element_roots,
                    );
                    styles.resolve_element_roots(
                        &invalidation.element_roots,
                        &mut invalidation_roots,
                        &mut element_roots,
                    );
                    let mut stats = styles.refresh_subtrees_after_invalidation(
                        &self.dom.document,
                        &invalidation_roots,
                        &element_roots,
                        &invalidation.removed_nodes,
                        invalidation.impact,
                    );
                    stats.invalidation_fallbacks = fallbacks;
                    stats
                } else {
                    StyleRefreshStats {
                        invalidated_nodes,
                        total_styles: styles.styles.len(),
                        ..StyleRefreshStats::default()
                    }
                };
                (styles, stats)
            }
            Some((_, _, mut styles)) => {
                // Changed rules restyle their subjects (`refresh_rules_after_invalidation`).
                // DOM changes made while styles were deferred arrive as the deferred
                // invalidation merged above.
                let stats = styles.refresh_rules_after_invalidation(
                    &self.dom,
                    &self.base_url,
                    &self.stylesheet_sources,
                    self.media_environment
                        .with_viewport(viewport_width, viewport_height),
                    invalidation,
                );
                (styles, stats)
            }
            None => {
                let build = if layout_only {
                    StyleSet::from_sources_for_layout
                } else {
                    StyleSet::from_sources_for_media_environment
                };
                let styles = build(
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
                        non_deferable_paint_changes: true,
                        full_rebuild: true,
                        ..StyleRefreshStats::default()
                    },
                )
            }
        }
    }
}
