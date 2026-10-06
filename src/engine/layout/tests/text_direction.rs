use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn text_direction_resolves_logical_alignment_but_preserves_physical_values() {
    for (direction, align, offset) in [
        ("ltr", "start", 0.),
        ("rtl", "start", 180.),
        ("ltr", "end", 180.),
        ("rtl", "end", 0.),
        ("rtl", "left", 0.),
        ("ltr", "right", 180.),
        ("rtl", "right", 180.),
        ("ltr", "left", 0.),
        ("ltr", "center", 90.),
        ("rtl", "center", 90.),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{width:200px;font-size:20px;direction:{direction};text-align:{align}}}</style><main>XX</main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800., 600., &mut FixedMeasurer);
        let (rect, font) = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text {
                    text, rect, font, ..
                } if text == "XX" => Some((rect, font)),
                _ => None,
            })
            .unwrap();
        assert!(
            (rect.x - offset).abs() < 0.001,
            "{direction}/{align}: {rect:?}"
        );
        assert_eq!(font.rtl, direction == "rtl");
    }
}

#[test]
fn text_direction_inherited_alignment_resolves_at_the_child_used_value() {
    let page = Page::parse(
        "<style>body{margin:0}main{direction:rtl;text-align:start}p{margin:0;width:200px;font-size:20px}#opposite{direction:ltr}</style><main><p>AA</p><p id=opposite>BB</p></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800., 600., &mut FixedMeasurer);
    for (wanted, x) in [("AA", 180.), ("BB", 0.)] {
        let rect = output
            .items
            .iter()
            .find_map(|item| match item {
                DisplayItem::Text { text, rect, .. } if text == wanted => Some(rect),
                _ => None,
            })
            .unwrap();
        assert_eq!(rect.x, x, "{wanted}");
    }
}
