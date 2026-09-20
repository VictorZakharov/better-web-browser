//! CSS text-truncation regression contracts: single-line `text-overflow` and the
//! legacy `-webkit-line-clamp` combination. These cases assert real engine
//! behavior (painted marker text, clamped box sizes, preserved DOM strings)
//! rather than parser acceptance alone.

use super::*;
use crate::engine::layout::test_support::{CountingMeasurer, FixedMeasurer};
mod regressions;

const VIEWPORT: (f32, f32) = (800.0, 600.0);
const LINE: f32 = 20.0;

fn render(html: &str) -> (Page, LayoutOutput) {
    let page = Page::parse(html, "https://example.test/");
    let output = layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut FixedMeasurer);
    (page, output)
}

fn painted_text(output: &LayoutOutput) -> Vec<(String, RectF, Option<String>)> {
    output
        .items
        .iter()
        .filter_map(|item| match item {
            DisplayItem::Text {
                text, rect, link, ..
            } => Some((text.clone(), *rect, link.clone())),
            _ => None,
        })
        .collect()
}

fn has_ellipsis(output: &LayoutOutput) -> bool {
    painted_text(output)
        .iter()
        .any(|(text, _, _)| text.ends_with('…'))
}

fn block_rect(page: &Page, output: &LayoutOutput, id: &str) -> RectF {
    let node = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    output.node_bounds[&node.id()]
}

fn single_page(style: &str, body: &str) -> String {
    format!(
        "<style>body{{margin:0}}p{{margin:0}}#box{{font:16px Arial;line-height:20px;{style}}}</style>\
         <div id=\"box\">{body}</div>"
    )
}

const LONG_TEXT: &str = "The quick brown fox jumps over the lazy dog near the riverbank at dusk while birds return home to rest.";
const SINGLE_STYLE: &str = "width:320px;white-space:nowrap;overflow:hidden;text-overflow:ellipsis;";
const CLAMP_BASE: &str = "width:320px;overflow:hidden;display:-webkit-box;\
    -webkit-box-orient:vertical;";

#[test]
fn single_line_ellipsis_marks_overflow() {
    let (_, output) = render(&single_page(SINGLE_STYLE, LONG_TEXT));
    let texts = painted_text(&output);
    assert!(!texts.is_empty());
    assert!(
        has_ellipsis(&output),
        "overflowing nowrap text must end with U+2026, got {texts:?}"
    );
    for (text, rect, _) in &texts {
        assert!(
            rect.right() <= 320.5,
            "ellipsized text must fit the content width, got {text:?} at {rect:?}"
        );
    }
}

#[test]
fn single_line_clip_adds_no_marker() {
    let html = single_page(
        "width:320px;white-space:nowrap;overflow:hidden;text-overflow:clip;",
        LONG_TEXT,
    );
    let (_, output) = render(&html);
    assert!(!has_ellipsis(&output));
    let joined: String = painted_text(&output)
        .iter()
        .map(|(text, _, _)| text.as_str())
        .collect();
    assert_eq!(joined, LONG_TEXT);
}

#[test]
fn single_line_short_and_exact_fit_text_stays_unchanged() {
    for body in ["Hi.", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"] {
        let (_, output) = render(&single_page(SINGLE_STYLE, body));
        assert!(
            !has_ellipsis(&output),
            "exact-fit text {body:?} gained a marker"
        );
        let texts = painted_text(&output);
        assert_eq!(texts.len(), 1);
        assert_eq!(texts[0].0, body);
    }
}

#[test]
fn single_line_narrower_than_marker_falls_back_to_clip() {
    // Chrome 153 paints the full text (visually clipped) when even the marker
    // cannot fit, instead of inventing a lone marker.
    for width in ["8px", "0px"] {
        let html = single_page(
            &format!("width:{width};white-space:nowrap;overflow:hidden;text-overflow:ellipsis;"),
            LONG_TEXT,
        );
        let (_, output) = render(&html);
        assert!(
            !has_ellipsis(&output),
            "narrow box {width} must not invent a marker"
        );
        let joined: String = painted_text(&output)
            .iter()
            .map(|(text, _, _)| text.as_str())
            .collect();
        assert_eq!(joined, LONG_TEXT);
    }
}

#[test]
fn single_line_respects_border_box_padding() {
    let html = single_page(
        "box-sizing:border-box;width:320px;padding:0 40px;white-space:nowrap;\
         overflow:hidden;text-overflow:ellipsis;",
        LONG_TEXT,
    );
    let (page, output) = render(&html);
    assert!(has_ellipsis(&output));
    let container = block_rect(&page, &output, "box");
    assert_eq!((container.x, container.width), (0.0, 320.0));
    for (text, rect, _) in painted_text(&output) {
        assert!(
            rect.right() <= container.x + 280.5,
            "padding must shrink the ellipsis budget: {text:?} at {rect:?}"
        );
    }
}

#[test]
fn single_line_keeps_link_and_mixed_fonts_on_marker() {
    let html = single_page(
        SINGLE_STYLE,
        "The quick <span>brown fox <a href=\"/fox\">jumps over the lazy dog near the</a></span>\
         <span style=\"font-size:24px\">riverbank at dusk and beyond</span>",
    );
    let (_, output) = render(&html);
    let texts = painted_text(&output);
    let marker = texts
        .iter()
        .find(|(text, _, _)| text.ends_with('…'))
        .expect("mixed-font overflow must carry a marker");
    assert_eq!(
        marker.2.as_deref(),
        Some("https://example.test/fox"),
        "marker keeps the truncated link target, got {texts:?}"
    );
    for (text, rect, _) in &texts {
        assert!(rect.right() <= 320.5, "{text:?} escapes the content box");
    }
}

#[test]
fn single_line_image_near_boundary_keeps_image_and_marker() {
    let html = single_page(
        SINGLE_STYLE,
        "The quick <span>brown fox <img id=\"pic\" src=\"pic.png\" width=\"40\" height=\"20\"> jumps over the lazy dog near the riverbank at dusk and beyond</span>.",
    );
    let mut page = Page::parse(&html, "https://example.test/");
    page.images.insert(
        "https://example.test/pic.png".into(),
        crate::engine::page::DecodedImage {
            width: 40,
            height: 20,
            bgra: vec![0; 40 * 20 * 4].into(),
        },
    );
    let mut measurer = FixedMeasurer;
    let output = layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut measurer);
    assert!(
        output
            .items
            .iter()
            .any(|item| matches!(item, DisplayItem::Image { .. })),
        "atomic image before the truncation point must survive"
    );
    assert!(has_ellipsis(&output), "text after the image must ellipsize");
}

#[test]
fn truncation_preserves_dom_strings() {
    let html = single_page(SINGLE_STYLE, LONG_TEXT);
    let (page, output) = render(&html);
    let boxed = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("box"))
        .unwrap();
    assert_eq!(boxed.text_content(), LONG_TEXT);
    assert!(has_ellipsis(&output));
}

#[test]
fn single_line_geometry_and_paint_agree() {
    let page = Page::parse(
        &single_page(SINGLE_STYLE, LONG_TEXT),
        "https://example.test/",
    );
    let painted = layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut FixedMeasurer);
    let geometry = layout_geometry_with_style_viewport(
        &page,
        VIEWPORT.0,
        VIEWPORT.1,
        VIEWPORT.0,
        &mut FixedMeasurer,
    );
    assert_eq!(painted.node_bounds, geometry.node_bounds);
    assert_eq!(painted.fragments, geometry.fragments);
}

#[test]
fn truncation_measurement_growth_stays_bounded() {
    // One unbreakable run keeps atom counts constant, so growth isolates ellipsis probing.
    let word = "a".repeat(2000);
    let page = Page::parse(&single_page(SINGLE_STYLE, &word), "https://example.test/");
    let mut small = CountingMeasurer::default();
    layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut small);
    let word = "a".repeat(4000);
    let page = Page::parse(&single_page(SINGLE_STYLE, &word), "https://example.test/");
    let mut large = CountingMeasurer::default();
    layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut large);
    assert!(
        large.calls <= small.calls + 24,
        "doubling ellipsized input must not explode measurement work: {} -> {}",
        small.calls,
        large.calls
    );
    // Clamped wrapping grows linearly with atom count; truncation probes must not make it quadratic.
    let style = format!("{CLAMP_BASE}-webkit-line-clamp:2;");
    let page = Page::parse(
        &single_page(&style, &LONG_TEXT.repeat(20)),
        "https://example.test/",
    );
    let mut small = CountingMeasurer::default();
    layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut small);
    let page = Page::parse(
        &single_page(&style, &LONG_TEXT.repeat(40)),
        "https://example.test/",
    );
    let mut large = CountingMeasurer::default();
    layout_page(&page, VIEWPORT.0, VIEWPORT.1, &mut large);
    assert!(
        f64::from(u32::try_from(large.calls).unwrap())
            <= 3.0 * f64::from(u32::try_from(small.calls).unwrap()) + 32.0,
        "doubling clamped input must stay far from quadratic: {} -> {}",
        small.calls,
        large.calls
    );
}

#[test]
fn clamp_counts_limit_visible_lines_with_marker() {
    // Two copies need about six wrapped lines at 320px, so counts 1-3 all cut.
    let body = LONG_TEXT.repeat(2);
    for lines in [1_u32, 2, 3] {
        let style = format!("{CLAMP_BASE}-webkit-line-clamp:{lines};");
        let (page, output) = render(&single_page(&style, &body));
        let container = block_rect(&page, &output, "box");
        assert_eq!(
            container.height,
            (lines as f32) * LINE,
            "clamp({lines}) must size the box to {lines} lines"
        );
        assert!(
            has_ellipsis(&output),
            "clamp({lines}) must mark the final visible line"
        );
    }
}

#[test]
fn clamp_exact_fit_and_none_gain_no_marker() {
    let html = single_page(
        &format!("{CLAMP_BASE}-webkit-line-clamp:2;"),
        "The quick brown fox.",
    );
    let (_, output) = render(&html);
    assert!(!has_ellipsis(&output));
    let joined: String = painted_text(&output)
        .iter()
        .map(|(text, _, _)| text.as_str())
        .collect();
    assert_eq!(joined, "The quick brown fox.");

    let html = single_page(&format!("{CLAMP_BASE}-webkit-line-clamp:none;"), LONG_TEXT);
    let (page, output) = render(&html);
    assert!(!has_ellipsis(&output));
    assert!(block_rect(&page, &output, "box").height > 2.0 * LINE);
}

#[test]
fn clamp_honors_breaks_and_nested_inline_content() {
    let html = single_page(
        &format!("{CLAMP_BASE}-webkit-line-clamp:2;"),
        "The quick <span>brown fox <a href=\"/fox\">jumps over</a></span> the lazy dog near the \
         <br>riverbank at dusk while birds return <span>home to rest beyond the line</span>.",
    );
    let (page, output) = render(&html);
    assert_eq!(block_rect(&page, &output, "box").height, 2.0 * LINE);
    assert!(has_ellipsis(&output));
}

#[test]
fn clamp_positions_following_sibling_at_clamped_size() {
    let html = format!(
        "<style>body{{margin:0}}p{{margin:0}}#box{{font:16px Arial;line-height:20px;{CLAMP_BASE}\
         -webkit-line-clamp:2;}}#after{{font:16px Arial;line-height:20px;}}</style>\
         <div id=\"box\">{LONG_TEXT}</div><p>Following sibling text.</p>"
    );
    let (page, output) = render(&html);
    let container = block_rect(&page, &output, "box");
    assert_eq!(container.height, 2.0 * LINE);
    let sibling = page.dom.elements_named("p").next().unwrap();
    assert_eq!(output.node_bounds[&sibling.id()].y, container.bottom());
}

#[test]
fn adjacent_clamps_stay_independent() {
    let html = format!(
        "<style>body{{margin:0}}#one,#two{{font:16px Arial;line-height:20px;{CLAMP_BASE}}}\
         #one{{-webkit-line-clamp:1;}}#two{{-webkit-line-clamp:3;}}</style>\
         <div id=\"one\">{LONG_TEXT}</div><div id=\"two\">{LONG_TEXT}</div>"
    );
    let (page, output) = render(&html);
    let one = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("one"))
        .unwrap();
    let two = page
        .dom
        .elements_named("div")
        .find(|node| node.attr("id").as_deref() == Some("two"))
        .unwrap();
    assert_eq!(output.node_bounds[&one.id()].height, LINE);
    assert_eq!(output.node_bounds[&two.id()].height, 3.0 * LINE);
    assert_eq!(
        output.node_bounds[&two.id()].y,
        output.node_bounds[&one.id()].bottom()
    );
}

#[test]
fn clamped_block_inside_flex_and_grid_items() {
    let html = format!(
        "<style>body{{margin:0}}#flex{{display:flex;}}#grid{{display:grid;}}\
         .inner{{font:16px Arial;line-height:20px;{CLAMP_BASE}-webkit-line-clamp:2;}}</style>\
         <div id=\"flex\"><div><div class=\"inner\" id=\"flex-box\">{LONG_TEXT}</div></div></div>\
         <div id=\"grid\"><div><div class=\"inner\" id=\"grid-box\">{LONG_TEXT}</div></div></div>"
    );
    let (page, output) = render(&html);
    for id in ["flex-box", "grid-box"] {
        let node = page
            .dom
            .elements_named("div")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap();
        assert_eq!(
            output.node_bounds[&node.id()].height,
            2.0 * LINE,
            "{id} must clamp inside its item"
        );
    }
    assert!(has_ellipsis(&output));
}

#[test]
fn removing_clamp_restores_full_content() {
    let clamped = single_page(&format!("{CLAMP_BASE}-webkit-line-clamp:2;"), LONG_TEXT);
    let (clamped_page, before) = render(&clamped);
    assert!(has_ellipsis(&before));
    let clamped_height = block_rect(&clamped_page, &before, "box").height;
    let restored = single_page("width:320px;overflow:hidden;", LONG_TEXT);
    let (page, after) = render(&restored);
    assert!(!has_ellipsis(&after));
    // Wrapped lines trim line-start spaces when painting, so compare geometry:
    // the restored box regains every line instead of two clamped ones.
    let restored_height = block_rect(&page, &after, "box").height;
    assert_eq!(clamped_height, 2.0 * LINE);
    assert!(restored_height > clamped_height);
}
