use super::*;
use crate::engine::layout::test_support::FixedMeasurer;
#[test]
fn masks_assemble_children_inside_a_stacking_context_without_changing_geometry() {
    let page = Page::parse(
        "<style>body{margin:0}#p{width:100px;height:50px;mask-image:linear-gradient(black,transparent);background:red;opacity:.5}#c{position:relative;z-index:-1;width:20px;height:20px;background:blue}</style><div id=p><div id=c></div></div>",
        "https://example.test/",
    );
    let layout = layout_page(&page, 300.0, 200.0, &mut FixedMeasurer);
    let begin = layout
        .items
        .iter()
        .position(|i| matches!(i, DisplayItem::BeginCssMask { .. }))
        .unwrap();
    let end = layout
        .items
        .iter()
        .position(|i| matches!(i, DisplayItem::EndMask { .. }))
        .unwrap();
    assert!(
        layout.items[..begin]
            .iter()
            .any(|i| matches!(i, DisplayItem::BeginOpacity { .. }))
    );
    assert!(
        layout.items[begin + 1..end].iter().any(
            |i| matches!(i,DisplayItem::SolidRect { color, .. } if *color==Color::rgb(0,0,255))
        )
    );
    let DisplayItem::BeginCssMask { bounds, layers } = &layout.items[begin] else {
        unreachable!()
    };
    assert_eq!((bounds.width, bounds.height), (100.0, 50.0));
    assert_eq!(layers.len(), 1);
    assert_eq!(layout.content_height, 200.0);
}
#[test]
fn two_none_layers_mask_but_single_none_does_not() {
    for (value, masked) in [("none", false), ("none,none", true)] {
        let page = Page::parse(
            &format!(
                "<div style='width:20px;height:20px;background:red;mask-image:{value}'></div>"
            ),
            "https://example.test/",
        );
        let layout = layout_page(&page, 300.0, 200.0, &mut FixedMeasurer);
        assert_eq!(
            layout
                .items
                .iter()
                .any(|i| matches!(i, DisplayItem::BeginCssMask { .. })),
            masked
        );
    }
}
