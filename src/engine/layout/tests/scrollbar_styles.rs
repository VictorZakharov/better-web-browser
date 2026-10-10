use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn hidden_scrollbars_reserve_no_gutter_but_preserve_scroll_and_wheel_reachability() {
    let page = Page::parse(
        "<style>body{margin:0}main{overflow:scroll;scrollbar-width:none;width:100px;height:40px}div{width:200px;height:100px}</style><main><div></div></main>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("main").next().unwrap();
    node.scroll_offset.set((50.0, 30.0));
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let scroll = output.scroll_boxes[&node.id()];
    assert_eq!((scroll.port.width, scroll.port.height), (100.0, 40.0));
    assert_eq!((scroll.offset_x, scroll.offset_y), (50.0, 30.0));
    assert!(scroll.can_scroll(1.0, 1.0));
    assert!(scroll.scrollbar(true).is_none() && scroll.scrollbar(false).is_none());
}

#[test]
fn thin_scrollbar_gutters_and_inherited_colors_are_actual_paint_not_detection_only() {
    let page = Page::parse(
        "<style>body{margin:0;scrollbar-color:red blue}main{overflow-y:scroll;scrollbar-width:thin;width:100px;height:40px}div{height:100px}</style><main><div></div></main>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("main").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let scroll = output.scroll_boxes[&node.id()];
    assert_eq!(scroll.port.width, 92.0);
    let (track, thumb) = scroll.scrollbar(true).unwrap();
    assert_eq!(track.width, 8.0);
    for (rect, color) in [
        (track, Color::rgb(0, 0, 255)),
        (thumb, Color::rgb(255, 0, 0)),
    ] {
        assert!(output.items.iter().any(|item| matches!(item, DisplayItem::SolidRect { rect: r, color: c, .. } if *r == rect && *c == color)));
    }
}
