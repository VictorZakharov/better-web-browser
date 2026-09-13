use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn element(page: &Page, id: &str) -> NodeRef {
    Node::composed_descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap()
}

#[test]
fn nested_scrollport_matches_chromium_for_direct_and_wrapped_sticky_boxes() {
    let page = Page::parse(
        include_str!("../../../../benchmarks/alpha/fixtures/scroll-containers.html"),
        "https://example.test/",
    );
    for id in ["wrapped", "direct"] {
        element(&page, id).scroll_offset.set((0.0, 150.0));
    }
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    for (id, y) in [("wrapped-sticky", 10.0), ("direct-sticky", 130.0)] {
        assert_eq!(
            output.visual_rect(&element(&page, id)).unwrap().width,
            185.0,
            "{id}"
        );
        assert_eq!(
            output.visual_rect(&element(&page, id)).unwrap().y,
            y,
            "{id}"
        );
    }
    assert_eq!(
        output.node_bounds[&element(&page, "wrapped-sticky").id()].y,
        0.0
    );
}

#[test]
fn viewport_sticky_changes_only_visual_geometry_and_reverses_without_drift() {
    let page = Page::parse(
        "<style>body{margin:0}main{margin-top:100px;height:500px}aside{position:sticky;top:24px;height:60px;background:red}</style><main><aside id=sticky></aside></main><div style='height:1000px'></div>",
        "https://example.test/",
    );
    let sticky = element(&page, "sticky");
    let mut output = layout_page(&page, 800.0, 200.0, &mut FixedMeasurer);
    let original = output.node_bounds.clone();
    let items = output.items.len();
    for (scroll, viewport_y) in [
        (0.0, 100.0),
        (150.0, 24.0),
        (550.0, -10.0),
        (150.0, 24.0),
        (0.0, 100.0),
    ] {
        page.dom.document.scroll_offset.set((0.0, scroll));
        output.update_sticky_positions(&page, 800.0, 200.0, 800.0);
        let rect = output.visual_rect(&sticky).unwrap();
        assert_eq!(rect.y - scroll, viewport_y);
        assert_eq!(output.node_bounds, original);
        assert_eq!(output.items.len(), items);
        assert!(output.items.iter().any(|item| matches!(item, DisplayItem::SolidRect{rect:paint,color,..} if *color==Color::rgb(255,0,0) && paint.y == rect.y)));
    }
}

#[test]
fn hidden_scroll_container_captures_sticky_but_clip_does_not() {
    for (overflow, expected) in [("hidden", 10.0), ("clip", 110.0)] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{overflow:{overflow};height:500px}}aside{{position:sticky;top:10px;height:20px}}</style><main><aside id=sticky></aside></main><div style='height:1000px'></div>"
            ),
            "https://example.test/",
        );
        page.dom.document.scroll_offset.set((0.0, 100.0));
        let output = layout_page(&page, 800.0, 200.0, &mut FixedMeasurer);
        assert_eq!(
            output.visual_rect(&element(&page, "sticky")).unwrap().y,
            expected,
            "{overflow}"
        );
    }
}

#[test]
fn sticky_insets_and_containing_block_limits() {
    assert_eq!(
        sticky_axis(
            0.0,
            20.0,
            0.0,
            500.0,
            100.0,
            100.0,
            Some(10.0),
            None,
            0.0,
            0.0
        ),
        110.0
    );
    assert_eq!(
        sticky_axis(
            200.0,
            20.0,
            0.0,
            500.0,
            0.0,
            100.0,
            None,
            Some(10.0),
            0.0,
            0.0
        ),
        -130.0
    );
    assert_eq!(
        sticky_axis(
            200.0,
            20.0,
            0.0,
            500.0,
            0.0,
            100.0,
            Some(10.0),
            None,
            0.0,
            0.0
        ),
        0.0
    );
    assert_eq!(
        sticky_axis(
            0.0,
            150.0,
            0.0,
            500.0,
            100.0,
            100.0,
            Some(10.0),
            Some(10.0),
            0.0,
            0.0
        ),
        110.0
    );
    assert_eq!(
        sticky_axis(
            0.0,
            20.0,
            0.0,
            100.0,
            200.0,
            100.0,
            Some(10.0),
            None,
            0.0,
            0.0
        ),
        80.0
    );
}
