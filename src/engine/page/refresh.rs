use super::*;
pub(super) mod profile;
mod resources;
mod unchanged;
use crate::engine::css::Display;
use crate::engine::dom::Node;
use crate::engine::invalidation::RenderInvalidation;
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
        let started = self.resource_profile.start();
        self.base_url = document_base_url(&self.dom, &self.source_url);
        self.media_environment = self
            .media_environment
            .with_viewport(viewport_width, viewport_height);
        // Script preparation belongs to the parser/runtime, not a rendering
        // checkpoint. Do not clone inline source only to discard it below.
        let resources = super::resources::discover_non_script_resources(
            &self.dom,
            &self.source_url,
            &self.base_url,
            self.media_environment,
        );
        for resource in resources {
            if !matches!(
                resource,
                PageResource::Script { .. } | PageResource::Media { .. }
            ) && !self.resources.contains(&resource)
            {
                self.resources.push(resource);
            }
        }
        self.resource_profile.finish(Phase::Discovery, started);
        let started = self.resource_profile.start();
        self.refresh_media_sources();
        self.resource_profile.finish(Phase::Media, started);
        let started = self.resource_profile.start();
        self.discover_stylesheet_dependencies();
        self.resource_profile.finish(Phase::Stylesheets, started);
        let started = self.resource_profile.start();
        let (styles, style_stats) =
            self.refresh_style_cache(viewport_width, viewport_height, invalidation, false);
        self.resource_profile.finish(Phase::Cascade, started);
        self.finish_refreshed_resources(styles, viewport_width, viewport_height);
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
            self.refresh_style_cache(viewport_width, viewport_height, invalidation, true);
        self.cached_styles = Some((viewport_width, viewport_height, styles));
        stats
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
                    styles.refresh_subtrees_after_invalidation(
                        &self.dom.document,
                        &invalidation_roots,
                        &invalidation.removed_nodes,
                        invalidation.impact,
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
            Some((_, _, mut styles)) => {
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
