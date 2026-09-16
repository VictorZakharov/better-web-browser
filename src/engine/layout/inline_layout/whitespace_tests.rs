use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

fn render(mode: &str, text: &str, width: f32) -> LayoutOutput {
    let page = Page::parse(
        &format!(
            "<style>body{{margin:0}}main{{width:{width}px;white-space:{mode};font-size:20px;line-height:24px}}</style><main>{text}</main>"
        ),
        "https://example.test/",
    );
    layout_page(&page, 800.0, 600.0, &mut FixedMeasurer)
}

fn lines(output: &LayoutOutput) -> Vec<(String, f32, f32)> {
    output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text { text, rect, .. } => Some((text.clone(), rect.x, rect.y)),
            _ => None,
        })
        .collect()
}

#[test]
fn pre_wrap_preserves_spaces_and_newlines_but_wraps_unlike_pre() {
    let wrapped = lines(&render("pre-wrap", "aa   bb cc\ndd", 70.0));
    assert_eq!(
        wrapped.iter().map(|r| r.0.as_str()).collect::<Vec<_>>(),
        ["aa   ", "bb ", "cc", "dd"]
    );
    assert_eq!(
        wrapped.iter().map(|r| r.2).collect::<Vec<_>>(),
        [2.0, 2.0, 26.0, 50.0]
    );
    let pre = lines(&render("pre", "aa   bb cc\ndd", 70.0));
    assert_eq!(pre.len(), 2);
    assert_eq!(pre[0].0, "aa   bb cc");
}

#[test]
fn trailing_pre_wrap_spaces_hang_and_long_words_do_not_emergency_wrap() {
    let result = lines(&render("pre-wrap", "aa bb    cc", 50.0));
    assert_eq!(
        result.iter().map(|r| r.2).collect::<Vec<_>>(),
        [2.0, 2.0, 26.0]
    );
    let result = lines(&render("pre-wrap", "aa abcdefgh", 50.0));
    assert_eq!(result[1].0, "abcdefgh");
    assert_eq!(result[1].2, 26.0);
}

#[test]
fn pre_wrap_break_survives_inline_boundaries_and_resize() {
    let narrow = lines(&render("pre-wrap", "aa <b>bb</b> cc", 50.0));
    assert_eq!(narrow[0].2, narrow[1].2);
    assert!(narrow.last().unwrap().2 > narrow[0].2);
    let wide = lines(&render("pre-wrap", "aa <b>bb</b> cc", 100.0));
    assert!(wide.iter().all(|r| r.2 == wide[0].2));
}

#[test]
fn pre_wrap_preserves_leading_spaces_and_empty_lines() {
    let result = lines(&render("pre-wrap", "  aa\n\nbb", 60.0));
    assert_eq!(result[0].0, "  ");
    assert_eq!(result[1].1, 20.0);
    assert_eq!(result.last().unwrap().2, 50.0);
}

#[test]
fn forced_line_end_spaces_hang_only_when_they_do_not_fit() {
    for (text, width, expected_x) in [("aa  ", 80.0, 40.0), ("aa       ", 50.0, 30.0)] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{width:{width}px;white-space:pre-wrap;text-align:right;font-size:20px}}</style><main>{text}</main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        assert_eq!(lines(&output)[0].1, expected_x, "{text:?}");
    }
}

#[test]
fn intrinsic_pre_wrap_width_excludes_trailing_spaces() {
    let page = Page::parse(
        "<style>body{margin:0}main{float:left;white-space:pre-wrap;font-size:20px}</style><main>aa bb     </main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let node = page.dom.elements_named("main").next().unwrap();
    assert_eq!(output.node_bounds[&node.id()].width, 50.0);
}

#[test]
fn collapsed_space_before_pre_wrap_keeps_its_position_and_source_style() {
    let result = lines(&render(
        "normal",
        "aa <span style='white-space:pre-wrap'>bb cc</span> dd",
        200.0,
    ));
    assert_eq!(
        result.iter().map(|r| r.0.as_str()).collect::<String>(),
        "aa bb cc dd"
    );
    assert_eq!(result.iter().find(|r| r.0 == "bb ").unwrap().1, 30.0);
}
