//! Patch an existing group without changing its bounds, children or paint order.
//! https://www.w3.org/TR/css-color-4/#transparency
use super::*;
use crate::engine::invalidation::RenderInvalidation;

#[cfg(test)]
mod tests;

impl LayoutOutput {
    /// The caller must exclude pending resources, viewport changes and frame layout.
    /// Validate the entire checkpoint before changing any retained paint item.
    pub(crate) fn try_refresh_opacity(
        &mut self,
        page: &Page,
        viewport: [f32; 3],
        invalidation: &RenderInvalidation,
        stats: &crate::engine::StyleRefreshStats,
    ) -> bool {
        if !stats.opacity_only
            || stats.changed_styles == 0
            || stats.full_rebuild
            || stats.layout_changed
            || stats.removed_styles != 0
            || !invalidation.impact.is_style_only()
            || invalidation.rebuild_style_rules
            || !invalidation.removed_nodes.is_empty()
        {
            return false;
        }
        let Some(styles) = page.cached_style_for_viewport(viewport[2], viewport[1]) else {
            return false;
        };
        let mut changes = Vec::new();
        let mut changed_nodes = HashSet::new();
        for (index, pair) in self.items.windows(2).enumerate() {
            let [
                DisplayItem::NodeBoundary {
                    node_id,
                    entering: true,
                },
                DisplayItem::BeginOpacity { opacity, .. },
            ] = pair
            else {
                continue;
            };
            let Some(style) = styles.styles.get(node_id) else {
                return false;
            };
            if style.opacity != *opacity {
                if style.opacity >= 1.0 {
                    return false;
                }
                changed_nodes.insert(*node_id);
                changes.push((index + 1, style.opacity));
            }
        }
        // Inline fragments and generated boxes have separate paint ownership. An
        // unrepresented changed owner, including a hidden box, takes normal layout.
        if changed_nodes.len() != stats.changed_styles {
            return false;
        }
        for (index, value) in changes {
            if let DisplayItem::BeginOpacity { opacity, .. } = &mut self.items[index] {
                *opacity = value;
            }
        }
        true
    }
}
