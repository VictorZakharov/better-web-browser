//! Reflow independent viewport-fixed boxes without rebuilding unrelated normal flow.
//! CSS Position 3 §2.1/§3.5: fixed boxes are out of flow, but auto insets depend on
//! static position and transformed/contained ancestors can change their containing block.
//! https://www.w3.org/TR/css-position-3/#fixed-pos
use super::*;
mod admission;
mod image_inputs;
mod splice;
pub(in crate::engine::layout) use admission::capture;
pub(super) use image_inputs::retain as retain_image_inputs;

#[cfg(test)]
mod tests;

const MAX_LAYERS: usize = 16;
const MAX_MEMBERS: usize = 4096;
const MAX_TOTAL_MEMBERS: usize = 16384;

pub(super) struct OutputRequest {
    pub emit_paint: bool,
    pub root: Option<NodeRef>,
    pub first_line: u64,
}
impl OutputRequest {
    pub fn paint() -> Self {
        Self {
            emit_paint: true,
            root: None,
            first_line: 0,
        }
    }
    pub fn geometry() -> Self {
        Self {
            emit_paint: false,
            ..Self::paint()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FixedLayers {
    viewport: [f32; 3],
    layers: HashMap<NodeId, FixedLayer>,
    image_inputs: image_inputs::ImageInputs,
}
impl FixedLayers {
    pub(super) fn new(width: f32, height: f32, style_width: f32) -> Self {
        Self {
            viewport: [width, height, style_width],
            layers: HashMap::new(),
            image_inputs: Default::default(),
        }
    }
}

#[derive(Clone, Debug)]
struct FixedLayer {
    dom_members: HashSet<NodeId>,
    elements: HashSet<NodeId>,
    members: HashSet<NodeId>,
    z_index: Option<i32>,
}

impl LayoutOutput {
    /// Returns false without changing the retained output unless every dirty root has
    /// an independent, anchored fixed owner. Unknown/mixed work takes ordinary layout.
    pub(crate) fn try_refresh_fixed_layers<M: TextMeasurer>(
        &mut self,
        page: &Page,
        viewport: [f32; 3],
        invalidation: &crate::engine::invalidation::RenderInvalidation,
        measurer: &mut M,
    ) -> bool {
        let local_content =
            invalidation.impact.is_local_content_only() && !invalidation.impact.is_style_only();
        if self.fixed_layers.viewport != viewport
            || self.fixed_layers.layers.is_empty()
            || !self.sticky_layers.is_empty()
            || !(invalidation.impact.is_style_only() || local_content)
            || (invalidation.impact.affects_intrinsic_size() && !local_content)
            || invalidation.rebuild_style_rules
            || !(invalidation.removed_nodes.is_empty()
                || local_content && invalidation.removals_are_local)
            || invalidation.affected_roots().next().is_none()
            || !self.fixed_layers.image_inputs.matches(page)
        {
            return false;
        }
        let Some(styles) = page.cached_style_for_viewport(viewport[2], viewport[1]) else {
            return false;
        };
        if !styles.can_scope_inline_style_to_target()
            || (local_content && !styles.can_scope_fixed_content(invalidation.impact))
            || Node::shadow_including_descendants(&page.dom.document).any(|node| {
                node.is_fullscreen()
                    || node.is_popover_open()
                    || matches!(node.tag_name(), Some("iframe" | "video" | "audio"))
            })
        {
            return false;
        }
        let mut owners = HashSet::new();
        for id in invalidation.affected_roots() {
            let Some((&owner, _)) = self
                .fixed_layers
                .layers
                .iter()
                .find(|(_, layer)| layer.members.contains(&id))
            else {
                return false;
            };
            owners.insert(owner);
        }
        if invalidation.removed_nodes.iter().any(|id| {
            !owners
                .iter()
                .any(|owner| self.fixed_layers.layers[owner].members.contains(id))
        }) {
            return false;
        }
        let mut replacements = Vec::new();
        let mut total_members: usize = self
            .fixed_layers
            .layers
            .values()
            .map(|layer| layer.members.len())
            .sum();
        let mut first_line = self.fragments.next_line;
        for owner in owners {
            let Some(root) = page.dom.find_node(owner) else {
                return false;
            };
            let old = &self.fixed_layers.layers[&owner];
            if styles.get(&root).z_index != old.z_index {
                return false;
            }
            let Some(item_range) = splice::item_range(&self.items, owner) else {
                return false;
            };
            let Some(node_range) = splice::node_range(&self.node_paint_order, &old.members) else {
                return false;
            };
            let layer = layout_page_for_output(
                page,
                viewport[0],
                viewport[1],
                viewport[2],
                measurer,
                OutputRequest {
                    emit_paint: true,
                    root: Some(root),
                    first_line,
                },
            );
            let Some(new) = layer.fixed_layers.layers.get(&owner) else {
                return false;
            };
            let members = if local_content {
                &new.members
            } else {
                &old.members
            };
            if (new.dom_members != old.dom_members && !local_content)
                || new.elements != old.elements
                || !layer.sticky_layers.is_empty()
                || layer.items.iter().any(|item| {
                    matches!(
                        item,
                        DisplayItem::Control(_) | DisplayItem::EmbeddedFrame { .. }
                    )
                })
                || layer.node_bounds.keys().any(|id| !members.contains(id))
            {
                return false;
            }
            total_members = total_members
                .saturating_sub(old.members.len())
                .saturating_add(new.members.len());
            if total_members > MAX_TOTAL_MEMBERS {
                return false;
            }
            first_line = layer.fragments.next_line;
            replacements.push((item_range, node_range, owner, layer));
        }
        // Every check/partial layout completes before mutation. Replacing back-to-front
        // keeps both paint and hit-order offsets stable across independent siblings.
        replacements.sort_by_key(|(items, _, _, _)| std::cmp::Reverse(items.start));
        for (items, nodes, owner, layer) in replacements {
            splice::replace(self, items, nodes, owner, layer);
        }
        image_inputs::retain(page, self);
        true
    }
}
