//! Repaint text runs whose color changed without laying the document out again.
//
// A class change that only recolors text (`.dark p{color:#333}`) leaves every box and line
// where it was, but layout was the only producer of display items, so the whole document was
// laid out again to change one field per run. Each text run records the node whose computed
// `color` it painted with; when the cascade reports that every changed style differs only in a
// color that nothing but text paints (`StyleRefreshStats::text_color_only`), the retained items
// take their owners' new colors instead, as do the underlines inline layout paints after them.
// Only class changes qualify: other mutations (text, links, image alt text) change items
// without changing a style.
// https://www.w3.org/TR/css-color-4/#currentcolor-color
use super::*;
use crate::engine::invalidation::RenderInvalidation;

#[cfg(test)]
mod tests;

impl LayoutOutput {
    /// The caller must exclude pending resources, viewport changes and frame layout. Validates
    /// every text run before changing any of them.
    pub(crate) fn try_refresh_text_color(
        &mut self,
        page: &Page,
        viewport: [f32; 3],
        invalidation: &RenderInvalidation,
        stats: &crate::engine::StyleRefreshStats,
    ) -> bool {
        if !stats.text_color_only
            || stats.changed_styles == 0
            || stats.full_rebuild
            || stats.layout_changed
            || stats.removed_styles != 0
            || !only_class_changes(invalidation)
        {
            return false;
        }
        let Some(styles) = page.cached_style_for_viewport(viewport[2], viewport[1]) else {
            return false;
        };
        let mut changes = Vec::new();
        for (index, item) in self.items.iter().enumerate() {
            let DisplayItem::Text {
                color,
                color_owner,
                font,
                ..
            } = item
            else {
                continue;
            };
            // Ellipsis markers and renderer-made text have no owner to read a color from.
            let Some(style) = color_owner.and_then(|owner| styles.styles.get(&owner)) else {
                return false;
            };
            if style.color == *color {
                continue;
            }
            changes.push((index, style.color));
            // Inline layout paints a run's underline right after it, in the run's color.
            if font.underline {
                match self.items.get(index + 1) {
                    Some(DisplayItem::SolidRect { color: line, .. }) if line == color => {
                        changes.push((index + 1, style.color));
                    }
                    _ => return false,
                }
            }
        }
        for (index, value) in changes {
            if let DisplayItem::Text { color, .. } | DisplayItem::SolidRect { color, .. } =
                &mut self.items[index]
            {
                *color = value;
            }
        }
        true
    }
}

fn only_class_changes(invalidation: &RenderInvalidation) -> bool {
    !invalidation.attribute_changes.is_empty()
        && invalidation
            .attribute_changes
            .iter()
            .all(|change| change.attributes.is_empty())
        && invalidation.roots.is_empty()
        && invalidation.child_list_roots.is_empty()
        && invalidation.element_roots.is_empty()
        && invalidation.removed_nodes.is_empty()
        && !invalidation.rebuild_style_rules
}
