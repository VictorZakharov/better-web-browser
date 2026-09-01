use super::test_support::FixedMeasurer;
use super::*;

#[test]
fn generated_content_participates_in_inline_flow_in_tree_order() {
    let page = Page::parse(
        r#"<style>
            p::before { content: "Before " attr(data-label) ": "; color: #c00 }
            p:after { content: " after"; color: #00c }
        </style><p data-label="value">body</p>"#,
        "https://example.com/",
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let painted = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<String>();

    assert_eq!(painted, "Before value: body after");
    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::Text { text, color, node_id: None, .. }
            if text.contains("Before") && *color == Color::rgb(204, 0, 0))
    }));
}

#[test]
fn empty_generated_content_can_paint_a_positioned_box() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            .card { position: relative; width: 200px; height: 100px }
            .card::after {
                content: ""; position: absolute; right: 0; bottom: 0;
                display: block; width: 20px; height: 10px; background: #0c0;
            }
        </style><div class="card"></div>"#,
        "https://example.com/",
    );
    let dom_ids = Node::descendants(&page.dom.document)
        .map(|node| node.id())
        .collect::<std::collections::HashSet<_>>();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);

    assert!(output.items.iter().any(|item| {
        matches!(item, DisplayItem::SolidRect { rect, color, .. }
            if *color == Color::rgb(0, 204, 0)
                && (rect.width - 20.0).abs() < 0.1
                && (rect.height - 10.0).abs() < 0.1)
    }));
    assert!(output.node_bounds.keys().all(|id| dom_ids.contains(id)));
    assert!(
        output
            .node_paint_order
            .iter()
            .all(|id| dom_ids.contains(id))
    );
}

#[test]
fn generated_boxes_become_flex_items_without_entering_the_dom() {
    let page = Page::parse(
        r#"<style>
            nav { display: flex }
            nav::before { content: "first"; width: 80px }
            nav::after { content: "last"; width: 40px }
        </style><nav><span>middle</span></nav>"#,
        "https://example.com/",
    );
    let before_parse_count = Node::descendants(&page.dom.document).count();
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, 800.0, 600.0, &mut measurer);
    let painted = output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, .. } => Some(text.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(painted, ["first", "middle", "last"]);
    assert_eq!(
        Node::descendants(&page.dom.document).count(),
        before_parse_count
    );
}
