//! Flex text must not measure source casing then paint a wider transformed run.
use super::*;
use crate::engine::layout::test_support::FixedMeasurer;

#[test]
fn transformed_text_controls_intrinsic_flex_width_without_changing_dom_text() {
    for (source, transform, expected) in [
        ("straße", "uppercase", "STRASSE"),
        ("İ", "lowercase", "i\u{307}"),
        ("ßeta", "capitalize", "SSeta"),
    ] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}main{{display:flex;width:300px}}span{{font-size:20px;text-transform:{transform}}}</style><main><span>{source}</span><b>x</b></main>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let span = page.dom.elements_named("span").next().unwrap();
        let text_width = expected.chars().count() as f32 * 10.0;
        assert_eq!(
            output.node_bounds[&span.id()].width,
            text_width,
            "{transform}"
        );
        assert_eq!(span.text_content(), source);
        assert!(
            output.items.iter().any(|item| matches!(item,
            DisplayItem::Text { text, .. } if &**text == expected)),
            "{transform}"
        );
    }
}

#[test]
fn fractional_shrink_to_fit_text_does_not_wrap_from_coordinate_roundoff() {
    let page = Page::parse(
        "<style>body{margin:0}main{display:flex;width:300px;padding-left:150.12345px}a{font-size:13.12345px}</style><main><a>Reset profile</a></main>",
        "https://example.test/",
    );
    let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
    let anchor = page.dom.elements_named("a").next().unwrap();
    assert!((output.node_bounds[&anchor.id()].height - 13.12345 * 1.2).abs() < 0.01);
}

#[test]
fn case_expansion_also_sizes_shrink_to_fit_floats_and_buttons() {
    for markup in ["<aside>straße</aside>", "<button>straße</button>"] {
        let page = Page::parse(
            &format!(
                "<style>body{{margin:0}}aside,button{{float:left;text-transform:uppercase;font:20px Arial;margin:0;padding:0;border:0}}</style>{markup}"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut FixedMeasurer);
        let node = page
            .dom
            .elements_named(if markup.starts_with("<aside") {
                "aside"
            } else {
                "button"
            })
            .next()
            .unwrap();
        assert_eq!(output.node_bounds[&node.id()].width, 70.0);
    }
}
