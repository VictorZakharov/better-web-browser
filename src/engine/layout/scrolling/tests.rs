use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn nested_scrollport_clips_paint_without_changing_offset_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}main{width:200px;height:100px;overflow:auto}div{height:400px;background:red}</style><main><div></div></main>",
        "https://example.test/",
    );
    let container = page.dom.elements_named("main").next().unwrap();
    let content = page.dom.elements_named("div").next().unwrap();
    container.scroll_offset.set((0.0, 50.0));
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let scroll = layout.scroll_boxes[&container.id()];
    assert_eq!(scroll.content_height, 400.0);
    assert_eq!(scroll.offset_y, 50.0);
    assert_eq!(layout.node_bounds[&content.id()].y, 0.0);
    assert_eq!(layout.visual_rect(&content).unwrap().y, -50.0);
    assert!(layout.point_in_scroll_clips(&content, 10.0, 50.0));
    assert!(!layout.point_in_scroll_clips(&content, 10.0, 150.0));
    assert_eq!(layout.content_height, 600.0);
    assert!(layout.items.iter().any(|item| matches!(item, DisplayItem::SolidRect{rect,color,..} if *color==Color::rgb(255,0,0) && rect.y == -50.0)));
}

#[test]
fn hidden_allows_programmatic_scrolling_but_clip_does_not() {
    for (overflow, expected, user) in [
        ("auto", 300.0, true),
        ("scroll", 315.0, true),
        ("hidden", 300.0, false),
        ("clip", 0.0, false),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{height:100px;overflow:{overflow}}}div{{height:400px}}</style><main><div></div></main>"
            ),
            "https://example.test/",
        );
        let container = page.dom.elements_named("main").next().unwrap();
        container.scroll_offset.set((0.0, 999.0));
        let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let scroll = layout.scroll_boxes[&container.id()];
        assert_eq!(scroll.offset_y, expected, "{overflow}");
        assert_eq!(scroll.user_y, user);
        assert_eq!(container.scroll_offset.get().1, expected);
    }
}

#[test]
fn inner_scrollable_content_does_not_enlarge_outer_scroll_range() {
    let page = Page::parse(
        "<style>body{margin:0}main{width:200px;height:100px;overflow:auto}section{height:80px;overflow:auto}div{height:400px}</style><main><section><div></div></section></main>",
        "https://example.test/",
    );
    let main = page.dom.elements_named("main").next().unwrap();
    let inner = page.dom.elements_named("section").next().unwrap();
    let layout = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    assert_eq!(layout.scroll_boxes[&main.id()].content_height, 100.0);
    assert_eq!(layout.scroll_boxes[&inner.id()].content_height, 400.0);
}

#[test]
fn classic_gutters_reserve_space_and_one_axis_can_induce_the_other() {
    let page = Page::parse(
        "<style>body{margin:0}main{width:200px;height:100px;overflow:auto}div{width:190px;height:400px}</style><main><div></div></main>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("main").next().unwrap();
    node.scroll_offset.set((10000.0, 10000.0));
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let scroll = output.scroll_boxes[&node.id()];
    assert_eq!((scroll.port.width, scroll.port.height), (185.0, 85.0));
    assert_eq!(node.scroll_offset.get(), (5.0, 315.0));
    assert_eq!(
        (
            output.resize_boxes[&node.id()].border_width,
            output.resize_boxes[&node.id()].border_height
        ),
        (200.0, 100.0)
    );
    assert_eq!(scroll.scrollbar(true).unwrap().0.x, 185.0);
}

#[test]
fn transformed_scrollport_moves_with_its_paint_and_hit_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}main{transform:translate(30px,40px);width:200px;height:100px;overflow:auto}div{height:400px}</style><main><div></div></main>",
        "https://example.test/",
    );
    let node = page.dom.elements_named("main").next().unwrap();
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let scroll = output.scroll_boxes[&node.id()];
    assert_eq!((scroll.port.x, scroll.port.y), (30.0, 40.0));
}
