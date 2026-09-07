//! Vertical viewport overflow is distinct from the root's normal-flow block height.
use super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn scrollable_overflow_bottom(&self, root: &NodeRef) -> f32 {
        // CSS Overflow: include descendant border boxes and visible overflow, including
        // absolute/relative positioning. Do not use paint bounds: shadows and other ink
        // overflow must not create document scrollbars.
        // https://www.w3.org/TR/css-overflow-3/#scrollable
        let mut bottom = 0.0_f32;
        let mut pending = vec![(root.clone(), f32::INFINITY)];
        while let Some((node, clip_bottom)) = pending.pop() {
            let style = self.styles.get(&node);
            if style.display == Display::None || style.position == Position::Fixed {
                // Viewport-fixed subtrees do not move with the document scrollport.
                continue;
            }
            let bounds = self.output.node_bounds.get(&node.id());
            if let Some(rect) = bounds {
                // Negative-side overflow is not reachable by downward viewport scrolling.
                if rect.right() > 0.0 && rect.bottom().is_finite() {
                    bottom = bottom.max(rect.bottom().min(clip_bottom));
                }
            }
            let child_clip = if style.overflow_hidden {
                bounds.map_or(clip_bottom, |rect| clip_bottom.min(rect.bottom()))
            } else {
                clip_bottom
            };
            pending.extend(
                self.box_children(&node)
                    .into_iter()
                    .map(|child| (child, child_clip)),
            );
        }
        bottom
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
