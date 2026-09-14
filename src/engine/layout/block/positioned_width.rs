//! CSS 2.2 10.3.7: auto positioned width stretches only with two definite insets.
//! https://www.w3.org/TR/CSS22/visudet.html#abs-non-replaced-width
use super::super::*;

impl<M: TextMeasurer> LayoutEngine<'_, M> {
    pub(super) fn positioned_auto_width(
        &mut self,
        node: &NodeRef,
        style: &ComputedStyle,
        basis: f32,
        insets: f32,
        margins: ResolvedEdges,
    ) -> f32 {
        let left = style.left.resolve(basis, style.font_size);
        let right = style.right.resolve(basis, style.font_size);
        let available =
            (basis - left.unwrap_or(0.0) - right.unwrap_or(0.0) - margins.horizontal()).max(0.0);
        if left.is_some() && right.is_some() {
            return available;
        }
        let (minimum, preferred) = self.intrinsic_content_widths(node, Some(basis));
        // Constraints are applied afterward by the shared used-width resolver, including
        // the two-inset stretch case. The containing block can be narrower than min-content.
        (preferred + insets).min(available.max(minimum + insets))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::layout::test_support::FixedMeasurer;

    fn capture(css: &str, content: &str) -> (Page, LayoutOutput) {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}#anchor{{position:relative;width:44px;height:30px}}#popup{{position:absolute;top:100%;left:0;padding:16px;background:white;{css}}}</style><div id=anchor><div id=popup>{content}</div></div><p style='background:blue'>article</p>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let geometry =
            layout_geometry_with_style_viewport(&page, 800.0, 600.0, 800.0, &mut FixedMeasurer);
        assert_eq!(output.node_bounds, geometry.node_bounds);
        (page, output)
    }

    fn popup(page: &Page, output: &LayoutOutput) -> RectF {
        let node = page
            .dom
            .elements_named("div")
            .find(|n| n.attr("id").as_deref() == Some("popup"))
            .unwrap();
        output.node_bounds[&node.id()]
    }

    #[test]
    fn auto_absolute_popup_preserves_its_minimum_content_width() {
        let (page, output) = capture(
            "max-width:200px;overflow:hidden;z-index:50",
            "<div style='min-width:200px'>Contents menu</div>",
        );
        let rect = popup(&page, &output);
        assert_eq!(rect.width, 232.0);
        assert_eq!(rect.y, 30.0);
    }

    #[test]
    fn auto_positioned_width_shrinks_to_preferred_or_available_width() {
        for (css, expected) in [
            ("", 264.0),
            ("left:760px", 64.0),
            ("left:auto;right:20px", 264.0),
        ] {
            let (page, output) = capture(
                &format!("position:fixed;{css}"),
                "word word word word word word",
            );
            assert_eq!(popup(&page, &output).width, expected, "{css}");
        }
    }

    #[test]
    fn two_inset_width_is_clamped_before_positioning_and_uses_viewport_for_fixed() {
        let (page, output) = capture(
            "position:fixed;left:10%;right:10%;max-width:300px;margin-left:5px;margin-right:7px",
            "word",
        );
        let rect = popup(&page, &output);
        assert_eq!(rect.width, 332.0);
        assert_eq!(rect.x, 85.0);
        let (page, output) = capture("left:0;right:0;min-width:100px;max-width:50px", "word");
        assert_eq!(popup(&page, &output).width, 132.0);
    }

    #[test]
    fn popup_background_paints_above_the_following_article() {
        let (_, output) = capture("width:200px;z-index:50", "Contents menu");
        let paint = |color| {
            output
                .items
                .iter()
                .position(|item| matches!(item, DisplayItem::SolidRect {color:c,..} if *c==color))
                .unwrap()
        };
        assert!(paint(Color::WHITE) > paint(Color::rgb(0, 0, 255)));
    }
}
