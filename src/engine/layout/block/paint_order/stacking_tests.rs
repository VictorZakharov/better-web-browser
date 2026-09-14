use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn capture(wrapper: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}}#wrapper{{{wrapper}}}#popup{{position:absolute;z-index:50;left:0;top:30px;width:200px;height:100px;background:white}}#article{{height:200px;background:blue}}#peer{{position:absolute;z-index:10;background:red;width:20px;height:20px}}</style><header><nav id=wrapper><div style='position:relative;width:44px;height:30px'><div id=popup>menu</div></div></nav></header><main id=article>article</main><aside id=peer></aside>"
        ),
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    (page, output)
}
fn solid(output: &LayoutOutput, color: Color) -> usize {
    output
        .items
        .iter()
        .position(|item| matches!(item, DisplayItem::SolidRect { color:c, .. } if *c==color))
        .unwrap()
}

#[test]
fn positioned_descendants_escape_inline_float_and_auto_positioned_ancestors() {
    for wrapper in [
        "display:inline-block",
        "float:left",
        "position:relative",
        "display:inline-flex",
    ] {
        let (page, output) = capture(wrapper);
        assert!(
            solid(&output, Color::WHITE) > solid(&output, Color::rgb(0, 0, 255)),
            "{wrapper}"
        );
        assert!(
            solid(&output, Color::WHITE) > solid(&output, Color::rgb(255, 0, 0)),
            "stack level: {wrapper}"
        );
        let popup = Node::descendants(&page.dom.document)
            .find(|n| n.attr("id").as_deref() == Some("popup"))
            .unwrap();
        assert_eq!(
            output.node_paint_order.last(),
            Some(&popup.id()),
            "hit order: {wrapper}"
        );
    }
}

#[test]
fn real_stacking_contexts_keep_high_level_descendants_inside() {
    for wrapper in [
        "position:relative;z-index:0",
        "opacity:.5",
        "transform:translateX(0)",
    ] {
        let (_, output) = capture(wrapper);
        assert!(
            solid(&output, Color::WHITE) < solid(&output, Color::rgb(255, 0, 0)),
            "{wrapper}"
        );
    }
}

#[test]
fn escaped_popup_keeps_its_ancestor_clip_balanced() {
    let (_, output) = capture("display:inline-block;overflow:hidden;width:100px;height:60px");
    let mut clips = 0;
    for item in output.items {
        match item {
            DisplayItem::BeginClip { .. } => clips += 1,
            DisplayItem::EndClip { .. } => {
                assert!(clips > 0);
                clips -= 1;
            }
            DisplayItem::SolidRect { color, .. } if color == Color::WHITE => assert!(clips > 0),
            _ => {}
        }
    }
    assert_eq!(clips, 0);
}

#[test]
fn auto_and_zero_have_distinct_negative_descendant_paint_and_hit_order() {
    for level in ["auto", "0"] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}#parent{{position:relative;z-index:{level};background:white;width:100px;height:100px}}#child{{position:absolute;z-index:-1;background:blue;width:100px;height:100px}}</style><main id=parent><aside id=child></aside></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(
            solid(&output, Color::WHITE) < solid(&output, Color::rgb(0, 0, 255)),
            level == "0"
        );
        let parent = page.dom.elements_named("main").next().unwrap().id();
        let child = page.dom.elements_named("aside").next().unwrap().id();
        let rank = |id| {
            output
                .node_paint_order
                .iter()
                .position(|n| *n == id)
                .unwrap()
        };
        assert_eq!(rank(parent) < rank(child), level == "0");
    }
}
