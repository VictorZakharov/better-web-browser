use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn layout(html: &str) -> LayoutOutput {
    let page = Page::parse(
        &format!("<style>body{{margin:0}}p{{margin:0}}</style>{html}"),
        "https://example.test/",
    );
    layout_page(&page, 600.0, 800.0, &mut FixedMeasurer)
}
fn solid(output: &LayoutOutput, color: Color) -> usize {
    output
        .items
        .iter()
        .position(|item| matches!(item, DisplayItem::SolidRect {color:actual,..} if *actual==color))
        .unwrap()
}
#[test]
fn block_backgrounds_and_borders_are_below_floats_but_inline_content_is_above() {
    let output = layout(
        "<aside style='float:right;width:200px;height:100px;background:red'></aside><section><p style='background:blue;border-bottom:2px solid green'>body text</p></section>",
    );
    let float = solid(&output, Color::rgb(255, 0, 0));
    assert!(solid(&output, Color::rgb(0, 0, 255)) < float);
    let border = output
        .items
        .iter()
        .position(|item| matches!(item,DisplayItem::BorderRect{color,..} if color.green==128))
        .unwrap();
    let text = output
        .items
        .iter()
        .position(|item| matches!(item, DisplayItem::Text { .. }))
        .unwrap();
    assert!(border < float && float < text);
    assert!(
        !output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::PaintBoundary { .. }))
    );
}
#[test]
fn positioned_groups_keep_their_css_levels_around_normal_backgrounds_and_floats() {
    let output = layout(
        "<div style='position:relative;background:white'><div style='position:absolute;z-index:-1;background:yellow;width:20px;height:20px'></div><aside style='float:right;width:20px;height:20px;background:red'></aside><p style='background:blue'>text</p><div style='position:absolute;z-index:1;background:green;width:20px;height:20px'></div></div>",
    );
    let colors = [
        Color::WHITE,
        Color::rgb(255, 255, 0),
        Color::rgb(0, 0, 255),
        Color::rgb(255, 0, 0),
        Color::rgb(0, 128, 0),
    ];
    let indices = colors.map(|color| solid(&output, color));
    assert!(
        indices.windows(2).all(|pair| pair[0] < pair[1]),
        "{indices:?}"
    );
}
#[test]
fn phase_reordering_preserves_balanced_clips_and_atomic_opacity() {
    let output = layout(
        "<aside style='float:right;width:100px;height:100px;background:red'></aside><section style='overflow:hidden;height:40px'><p style='background:blue'>clipped text</p></section><div style='opacity:.5;background:green'><p style='background:yellow'>group</p></div>",
    );
    let mut clips = 0;
    let mut opacity = 0;
    for item in &output.items {
        match item {
            DisplayItem::BeginClip { .. } => clips += 1,
            DisplayItem::EndClip { .. } => {
                assert!(clips > 0);
                clips -= 1
            }
            DisplayItem::BeginOpacity { .. } => opacity += 1,
            DisplayItem::EndOpacity { .. } => {
                assert!(opacity > 0);
                opacity -= 1
            }
            DisplayItem::SolidRect { color, .. } if *color == Color::rgb(0, 0, 255) => {
                assert!(clips > 0)
            }
            DisplayItem::SolidRect { color, .. } if *color == Color::rgb(255, 255, 0) => {
                assert_eq!(opacity, 1)
            }
            _ => {}
        }
    }
    assert_eq!((clips, opacity), (0, 0));
}
