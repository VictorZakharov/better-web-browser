use super::super::super::test_support::FixedMeasurer;
use super::super::super::*;

#[test]
fn overflow_hidden_wraps_descendant_paint_in_the_padding_box() {
    let page = Page::parse(
        r#"<style>
            body { margin: 0 }
            #clip { width: 100px; height: 40px; padding: 4px; border: 2px solid black;
                    overflow: hidden }
            #wide { width: 200px; height: 20px; background: red }
        </style>
        <div id=clip><div id=wide></div></div>"#,
        "https://example.com/",
    );
    let clip = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("clip"))
        .unwrap();
    let wide = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("wide"))
        .unwrap();

    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let clip_rect = output.node_bounds[&clip.id()];
    let wide_rect = output.node_bounds[&wide.id()];
    let expected = RectF {
        x: clip_rect.x + 2.0,
        y: clip_rect.y + 2.0,
        width: clip_rect.width - 4.0,
        height: clip_rect.height - 4.0,
    };
    let begin = output
        .items
        .iter()
        .position(|item| matches!(item, DisplayItem::BeginClip { bounds } if *bounds == expected))
        .unwrap();
    let end = output
        .items
        .iter()
        .position(|item| matches!(item, DisplayItem::EndClip { bounds } if *bounds == expected))
        .unwrap();
    let wide_paint = output
        .items
        .iter()
        .position(|item| {
            matches!(item, DisplayItem::SolidRect { rect, color, .. }
            if *rect == wide_rect && *color == Color::rgb(255, 0, 0))
        })
        .unwrap();

    assert!(wide_rect.right() > expected.right());
    assert!(begin < wide_paint && wide_paint < end);
}
