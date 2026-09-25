use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn cell_alignment_moves_content_geometry_without_moving_the_cell_background() {
    for (alignment, offset) in [("top", 0.0), ("middle", 40.0), ("bottom", 80.0)] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}table{{width:200px;border-spacing:0}}td{{padding:0;border:0;height:100px;vertical-align:{alignment}}}div{{height:20px}}</style><table><tr><td><div>content</div></td></tr></table>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 400.0, 300.0, &mut FixedMeasurer);
        let geometry =
            layout_geometry_with_style_viewport(&page, 400.0, 300.0, 400.0, &mut FixedMeasurer);
        assert_eq!(output.node_bounds, geometry.node_bounds);
        assert_eq!(output.resize_boxes, geometry.resize_boxes);
        let cell = page.dom.elements_named("td").next().unwrap();
        let content = page.dom.elements_named("div").next().unwrap();
        assert_eq!(output.node_bounds[&cell.id()].y, 0.0);
        assert_eq!(output.node_bounds[&cell.id()].height, 100.0);
        assert_eq!(output.node_bounds[&content.id()].y, offset);
        let text_y = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text { rect, .. } => Some(rect.y),
                _ => None,
            })
            .unwrap();
        assert!(text_y >= offset && text_y < offset + 20.0);
    }
}

#[test]
fn table_ua_alignment_inheritance_and_author_css_wide_values_are_not_global_inheritance() {
    let page = Page::parse(
        "<table><tbody style='vertical-align:bottom'><tr><td id=inherited></td><td id=reset style='vertical-align:initial'></td><td id=revert style='vertical-align:top;vertical-align:revert'></td><td id=top valign=bottom style='vertical-align:top'><div></div></td></tr></tbody></table>",
        "https://example.test/",
    );
    let styles = page.style_for_viewport(400.0, 300.0);
    for (id, expected) in [
        ("inherited", VerticalAlign::Bottom),
        ("reset", VerticalAlign::Baseline),
        ("revert", VerticalAlign::Bottom),
        ("top", VerticalAlign::Top),
    ] {
        let node = Node::descendants(&page.dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap();
        let style = styles.get(&node);
        assert_eq!(style.vertical_align, expected, "{id}");
        assert_eq!(
            crate::engine::css::resolved_property_value(style, "vertical-align").as_deref(),
            Some(expected.css_keyword())
        );
        let mut changed = style.clone();
        changed.vertical_align = VerticalAlign::Middle;
        assert!(!style.layout_equivalent(&changed));
    }
    let div = page.dom.elements_named("div").next().unwrap();
    assert_eq!(styles.get(&div).vertical_align, VerticalAlign::Baseline);
}
