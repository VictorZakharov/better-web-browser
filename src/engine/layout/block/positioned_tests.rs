use super::super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn bounds(source: &str, id: &str) -> Option<RectF> {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}p{{margin:0}}</style>{source}"),
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let node = Node::descendants(&page.dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds.get(&node.id()).copied()
}

#[test]
fn nested_absolute_boxes_resolve_against_the_positioned_ancestors_specified_height() {
    // CSS 2.1 §10.1: the containing block is the padding box of the nearest positioned
    // ancestor, even when a static block sits in between.
    let rect = bounds(
        "<div style='position:relative;height:200px;margin-top:10px'><div>\
         <span id=a style='position:absolute;bottom:0;height:50%;width:10px'></span>\
         </div></div>",
        "a",
    )
    .unwrap();
    assert_eq!((rect.y, rect.height), (110.0, 100.0));
}

#[test]
fn absolute_boxes_in_inlines_that_need_their_static_position_are_not_moved_to_the_block_start() {
    // The static position inside a line box is not recorded, so a box with auto insets is
    // not laid out at the paragraph's top-left corner in its place.
    let rect = bounds(
        "<p>words before <span>x<span id=b style='position:absolute'>badge</span></span></p>",
        "b",
    );
    assert!(rect.is_none_or(|rect| rect.x > 0.0), "{rect:?}");
}
