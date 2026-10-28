//! Vertical viewport overflow is distinct from the root's normal-flow block height.
//
// The walk visits every box, so on a pass that replays most of the page it would cost as much as
// the replay saved. A replayed subtree is its retained layout moved as a whole, and every later
// move of an ancestor moves all of its boxes too, so its extent below its own border box is that
// of the retained layout. The walk keeps that extent per retained layout (`OverflowExtent`) and
// stops at a replayed root that has one. Clipping composes with it: an ancestor clip `c` limits
// each counted bottom `b` to `min(b, c)`, and the largest of those is `min(max b, c)`. Boxes
// left of the viewport do not count, so a horizontal move measures the subtree again.
use super::*;
use crate::engine::css::Overflow;
use engine::reuse::OverflowExtent;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn scrollable_overflow_bottom(&self, root: &NodeRef) -> f32 {
        self.overflow_bottom_below(root).max(0.0)
    }

    /// The lowest scrollable-overflow bottom in `root`'s subtree, unclipped by its ancestors;
    /// negative infinity when nothing counts.
    fn overflow_bottom_below(&self, root: &NodeRef) -> f32 {
        // CSS Overflow: include descendant border boxes and visible overflow, including
        // absolute/relative positioning. Do not use paint bounds: shadows and other ink
        // overflow must not create document scrollbars.
        // https://www.w3.org/TR/css-overflow-3/#scrollable
        let mut bottom = f32::NEG_INFINITY;
        let mut pending = vec![(root.clone(), f32::INFINITY)];
        while let Some((node, clip_bottom)) = pending.pop() {
            let style = self.styles.get(&node);
            if style.display == Display::None || style.position == Position::Fixed {
                // Viewport-fixed subtrees do not move with the document scrollport.
                continue;
            }
            let bounds = self.output.node_bounds.get(&node.id());
            if node.id() != root.id()
                && let Some(extent) = self.replayed_overflow(&node, bounds)
            {
                bottom = bottom.max(extent.min(clip_bottom));
                continue;
            }
            // Negative-side overflow is not reachable by downward viewport scrolling.
            let counted = bounds.filter(|rect| rect.right() > 0.0 && rect.bottom().is_finite());
            if let Some(rect) = counted {
                bottom = bottom.max(rect.bottom().min(clip_bottom));
            }
            // Root/body overflow propagates to the viewport. In particular,
            // overflow-x:hidden can compute overflow-y to auto; the latter
            // must retain the document's vertical scrollable extent.
            // https://drafts.csswg.org/css-overflow-3/#overflow-viewport
            let clips_descendants = if matches!(node.tag_name(), Some("html" | "body")) {
                matches!(style.overflow_axes().1, Overflow::Hidden | Overflow::Clip)
            } else {
                style.overflow_axes().1 != Overflow::Visible
            };
            let child_clip = if clips_descendants {
                if counted.is_some() {
                    // Descendants are clipped to a bottom this box already counted.
                    continue;
                }
                bounds.map_or(clip_bottom, |rect| clip_bottom.min(rect.bottom()))
            } else {
                clip_bottom
            };
            // Native select contents belong to its picker (see `box_children`).
            if node.tag_name() == Some("select") {
                continue;
            }
            let child = |child: &NodeRef| (child.clone(), child_clip);
            match self.styles.retained_children(&node) {
                Some(children) => pending.extend(children.iter().map(child)),
                None => pending.extend(self.styles.children(&node).iter().map(child)),
            }
        }
        bottom
    }

    /// The overflow bottom of a subtree this output placed by replaying a retained layout,
    /// measured once per retained layout.
    fn replayed_overflow(&self, node: &NodeRef, bounds: Option<&RectF>) -> Option<f32> {
        let serial = *self.replayed_roots.get(&node.id())?;
        let rect = bounds?;
        let mut reuse = self.page.layout_reuse.borrow_mut();
        let retained = match reuse.overflow.get(&node.id()) {
            Some(extent) => Some(*extent),
            None => reuse.lookup_in_source(|source, _| source.overflow.get(&node.id()).copied()),
        };
        drop(reuse);
        if let Some(extent) = retained
            && extent.serial == serial
            && extent.x == rect.x
        {
            return Some(rect.y + extent.below_top);
        }
        let bottom = self.overflow_bottom_below(node);
        self.page.layout_reuse.borrow_mut().overflow.insert(
            node.id(),
            OverflowExtent {
                serial,
                x: rect.x,
                below_top: bottom - rect.y,
            },
        );
        Some(bottom)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    fn height(markup: &str) -> f32 {
        let page = Page::parse(
            &format!("<style>body{{margin:0}}</style>{markup}"),
            "https://example.com/",
        );
        layout_page(&page, 800.0, 600.0, &mut FixedMeasurer).content_height
    }

    #[test]
    fn absolute_application_extends_the_viewport_beyond_its_empty_body() {
        assert_eq!(
            height("<main style='position:absolute;top:0;height:1800px;width:700px'></main>"),
            1800.0
        );
    }

    #[test]
    fn absolute_descendants_of_a_short_positioned_box_extend_document_overflow() {
        assert_eq!(
            height(
                "<main style='position:relative;height:40px'><div style='position:absolute;top:900px;height:200px;width:100px'></div></main>"
            ),
            1100.0
        );
    }

    #[test]
    fn visible_overflow_from_height_constrained_normal_flow_is_scrollable() {
        assert_eq!(
            height("<main style='height:40px'><div style='height:1200px'></div></main>"),
            1200.0
        );
    }

    #[test]
    fn clipped_descendants_do_not_extend_the_document() {
        assert_eq!(
            height(
                "<main style='height:40px;overflow:hidden'><div style='height:1200px'></div></main>"
            ),
            600.0
        );
    }

    #[test]
    fn horizontal_root_clipping_preserves_vertical_viewport_scroll_range() {
        assert_eq!(
            height(
                "<style>html{height:100%;overflow-x:hidden;overflow-y:auto}body{overflow-x:hidden}</style><main style='height:1200px'></main>"
            ),
            1200.0
        );
    }

    #[test]
    fn hidden_root_vertical_overflow_does_not_extend_viewport_scroll_range() {
        assert_eq!(
            height(
                "<style>html{height:100%;overflow-y:hidden}</style><main style='height:1200px'></main>"
            ),
            600.0
        );
    }

    #[test]
    fn descendants_of_clipping_boxes_reach_at_most_the_box_bottom() {
        assert_eq!(
            height(
                "<main style='height:900px;overflow:hidden'><div style='height:1200px'></div></main><div style='position:absolute;top:0;height:700px;overflow:auto'><p style='height:1000px'></p></div>"
            ),
            900.0
        );
        // A clipping box left of the viewport is not counted, but its clipped content is.
        assert_eq!(
            height(
                "<main style='position:absolute;left:-200px;width:100px;height:1000px;overflow:hidden'><div style='margin-left:300px;width:50px;height:1500px'></div></main>"
            ),
            1000.0
        );
    }

    #[test]
    fn viewport_fixed_descendants_do_not_create_document_scroll_range() {
        assert_eq!(
            height(
                "<main style='position:fixed;top:0;height:1800px;width:100px'><div style='height:2500px'></div></main>"
            ),
            600.0
        );
    }

    #[test]
    fn relative_offset_extends_scroll_range_without_changing_normal_flow() {
        assert_eq!(
            height("<main style='position:relative;top:800px;height:100px'></main>"),
            900.0
        );
    }
}
