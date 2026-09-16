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
