//! Paint elision must not mutate source geometry or depend on inline wrappers.
use super::*;
use unicode_segmentation::UnicodeSegmentation;

const WORD: &str = "ABCDEFGHIJKLMNO";
const STYLE: &str = "width:80px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";

fn visible(output: &LayoutOutput) -> String {
    painted_text(output)
        .into_iter()
        .map(|(text, _, _)| text)
        .collect()
}

#[test]
fn unicode_prefix_keeps_original_source_offsets_without_panicking() {
    // Removing three trailing ASCII bytes must not slice three bytes into 𝒜.
    let (page, output) = render(&single_page(
        "width:104px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;",
        "𝒜ABCDEFGHIJKLMN",
    ));
    assert!(has_ellipsis(&output));
    let boxed = page.dom.elements_named("div").next().unwrap();
    let text = boxed.children.borrow()[0].id();
    let first = output.fragments.range_text(text, 0, 2);
    assert_eq!(first.len(), 1);
    assert_eq!((first[0].x, first[0].width), (0.0, 8.0));
    assert_eq!(output.fragments.range_text(text, 15, 16)[0].x, 112.0);
}

#[test]
fn ellipsis_preserves_all_ranges_and_scroll_extent() {
    for body in [
        WORD.to_string(),
        format!("<span style='background:red'>{WORD}</span>"),
    ] {
        let mut results = Vec::new();
        for overflow in ["clip", "ellipsis"] {
            let (page, output) = render(&single_page(
                &format!("{STYLE}text-overflow:{overflow};"),
                &body,
            ));
            let boxed = page.dom.elements_named("div").next().unwrap();
            let text = Node::descendants(&boxed)
                .find(|node| matches!(node.data, NodeData::Text(_)))
                .unwrap();
            let ranges: Vec<_> = (0..15)
                .map(|i| output.fragments.range_text(text.id(), i, i + 1))
                .collect();
            assert!(ranges.iter().all(|rects| rects.len() == 1));
            results.push((ranges, output.scroll_boxes[&boxed.id()].content_width));
        }
        assert_eq!(results[0], results[1]);
        assert_eq!(results[1].1, 120.0);
    }
}

#[test]
fn nested_inline_wrappers_do_not_reserve_extra_markers() {
    let (_, plain) = render(&single_page(STYLE, WORD));
    for depth in 1..5 {
        let body = format!(
            "{}{}{}",
            "<span style='background:#ddd'>".repeat(depth),
            WORD,
            "</span>".repeat(depth)
        );
        let (_, nested) = render(&single_page(STYLE, &body));
        assert_eq!(visible(&nested), visible(&plain), "depth {depth}");
    }
}

#[test]
fn normally_wrapping_unbreakable_word_gets_an_ellipsis() {
    let (_, output) = render(&single_page(&format!("{STYLE}white-space:normal;"), WORD));
    assert_eq!(visible(&output), "ABCDEFGHI…");
}

#[test]
fn marker_uses_block_font_and_color_not_last_run() {
    let (_, output) = render(&single_page(
        &format!("{STYLE}color:red;"),
        "<span style='font-size:24px;color:blue'>ABCDEFGHIJKLMNO</span>",
    ));
    let (font, color) = output
        .items
        .iter()
        .find_map(|item| match item {
            DisplayItem::Text {
                text, font, color, ..
            } if text == "…" => Some((font, color)),
            _ => None,
        })
        .unwrap();
    assert_eq!(font.size, 16.0);
    assert_eq!(*color, Color::rgb(255, 0, 0));
    assert_eq!(visible(&output), "ABCDEF…");
}

#[test]
fn painted_prefix_ends_on_a_source_grapheme_boundary() {
    for body in [
        "e\u{301}ABCDEe\u{301}FGHIJKLMNO",
        "A👨‍👩‍👧BCDEFGHIJKLMNO",
        "𝒜ABCDEFGHIJKLMNO",
    ] {
        for width in [48, 64, 80, 96] {
            let (_, output) = render(&single_page(&format!("{STYLE}width:{width}px;"), body));
            let painted = visible(&output);
            let prefix = painted.strip_suffix('…').expect("must actually truncate");
            assert!(!prefix.is_empty());
            assert!(body.starts_with(prefix));
            assert!(
                body.grapheme_indices(true)
                    .any(|(byte, _)| byte == prefix.len()),
                "{painted:?}"
            );
        }
    }
}
