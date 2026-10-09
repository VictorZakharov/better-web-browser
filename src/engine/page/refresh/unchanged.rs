//! Inline CSS changes need a cascade, but exact unchanged output needs no
//! resource rediscovery or box reconstruction. This is not paint equivalence.
use super::*;
use crate::engine::dom::NodeData;
#[cfg(test)]
mod tests;

impl Page {
    pub(crate) fn refresh_presentation_styles(
        &mut self,
        width: f32,
        height: f32,
        invalidation: &RenderInvalidation,
    ) -> (StyleRefreshStats, bool) {
        if !self.can_refresh_style_only(width, height, invalidation) {
            return (
                self.refresh_resources_after_invalidation_for_viewport(width, height, invalidation),
                false,
            );
        }
        self.resource_profile.reset();
        let started = self.resource_profile.start();
        let (styles, stats) = self.refresh_style_cache(width, height, invalidation, false);
        self.resource_profile.finish(Phase::Cascade, started);
        let unchanged = stats.changed_styles == 0
            && stats.removed_styles == 0
            && !stats.layout_changed
            && !stats.non_deferable_paint_changes
            && !stats.full_rebuild;
        if unchanged {
            self.cached_styles = Some((width.max(1.0), height.max(1.0), styles));
        } else {
            // Do not refresh the cascade twice: that would erase the first
            // comparison's evidence (e.g. hiding a previously visible box).
            let started = self.resource_profile.start();
            let resources = super::super::resources::discover_non_script_resources(
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
            self.finish_refreshed_resources(styles, width, height);
        }
        (stats, unchanged)
    }

    fn can_refresh_style_only(
        &self,
        width: f32,
        height: f32,
        invalidation: &RenderInvalidation,
    ) -> bool {
        if !invalidation.impact.is_style_only()
            || invalidation.rebuild_style_rules
            || invalidation.roots.is_empty()
            || !invalidation.removed_nodes.is_empty()
            || self
                .cached_styles
                .as_ref()
                .is_none_or(|(old_width, old_height, styles)| {
                    *old_width != width.max(1.0)
                        || *old_height != height.max(1.0)
                        || !styles.can_prove_unchanged_presentation_styles()
                })
        {
            return false;
        }
        invalidation.roots.iter().all(|id| {
            self.dom.find_node(*id).is_some_and(|root| {
                Node::shadow_including_descendants(&root).all(|node| ordinary_html(&node))
                    && std::iter::successors(root.shadow_including_parent(), |node| {
                        node.shadow_including_parent()
                    })
                    .all(|node| ordinary_html(&node))
            })
        })
    }
}

fn ordinary_html(node: &NodeRef) -> bool {
    !node.is_fullscreen()
        && node.shadow_root().is_none()
        && !matches!(node.data, NodeData::ShadowRoot(_))
        && !matches!(node.tag_name(), Some("base" | "slot"))
        && node
            .namespace_uri()
            .is_none_or(|namespace| namespace == "http://www.w3.org/1999/xhtml")
}
