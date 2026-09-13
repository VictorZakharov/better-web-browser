use super::*;

fn computed(source: &str, id: &str) -> ComputedStyle {
    let dom = dom::parse(source);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom::Node::descendants(&dom.document)
        .find(|node| node.attr("id").as_deref() == Some(id))
        .unwrap();
    styles.get(&node).clone()
}

#[test]
fn unitless_inheritance_scales_but_computed_lengths_do_not() {
    for (value, expected) in [
        ("1.5", 60.0),
        ("150%", 30.0),
        ("1.5em", 30.0),
        ("30px", 30.0),
        ("normal", 48.0),
    ] {
        let source = format!(
            "<div style='font-size:20px;line-height:{value}'><section><span id=child style='font-size:40px'></span></section></div>"
        );
        assert_eq!(computed(&source, "child").line_height, expected, "{value}");
    }
}

#[test]
fn font_size_and_shorthand_obey_declaration_order_and_importance() {
    for (declarations, expected) in [
        ("font-size:20px;line-height:150%", 30.0),
        ("line-height:150%;font-size:20px", 30.0),
        ("line-height:40px;font:20px/1.5 Arial", 30.0),
        ("font:20px/1.5 Arial;line-height:40px", 40.0),
        ("line-height:40px!important;font:20px/1.5 Arial", 40.0),
        ("font:20px/1.5 Arial!important;line-height:40px", 30.0),
        ("font:20px/150% Arial;font-size:40px", 60.0),
        ("line-height:40px;font:20px Arial", 24.0),
    ] {
        assert_eq!(
            computed(
                &format!("<div id=child style='{declarations}'></div>"),
                "child"
            )
            .line_height,
            expected,
            "{declarations}"
        );
    }
}

#[test]
fn css_wide_keywords_preserve_the_computed_kind() {
    for (value, expected) in [
        ("inherit", 60.0),
        ("unset", 60.0),
        ("initial", 48.0),
        ("revert", 60.0),
    ] {
        let source = format!(
            "<div style='font-size:20px;line-height:1.5'><span id=child style='font-size:40px;line-height:{value}'></span></div>"
        );
        assert_eq!(computed(&source, "child").line_height, expected, "{value}");
    }
}

#[test]
fn font_shorthand_slash_whitespace_and_invalid_line_height_are_atomic() {
    for value in [
        "20px/1.5 Arial",
        "20px /1.5 Arial",
        "20px/ 1.5 Arial",
        "20px / 1.5 Arial",
        "20px /* before slash */ / /* after slash */ 1.5 Arial",
        "20px / calc(10px + 1em) Arial",
    ] {
        assert_eq!(
            computed(
                &format!("<div id=child style='font:{value}'></div>"),
                "child"
            )
            .line_height,
            30.0,
            "{value}"
        );
    }
    let style = computed(
        "<div id=child style='font:16px/32px Arial;font:bold 20px/-1 Arial'></div>",
        "child",
    );
    assert_eq!(style.font_size, 16.0);
    assert_eq!(style.line_height, 32.0);
    assert_eq!(style.font_weight, 400);
}

#[test]
fn short_zero_and_invalid_values_are_not_expanded_to_the_font_size() {
    for (value, expected) in [
        ("0", 0.0),
        ("0px", 0.0),
        ("0.25", 4.0),
        ("8px", 8.0),
        ("50%", 8.0),
    ] {
        assert_eq!(
            computed(
                &format!("<span id=child style='font-size:16px;line-height:{value}'></span>"),
                "child"
            )
            .line_height,
            expected,
            "{value}"
        );
    }
    for value in ["-1", "-1px", "-1%", "NaN", "inf", "auto", "1 2"] {
        assert!(LineHeight::parse(value).is_none(), "{value}");
        assert_eq!(
            computed(
                &format!("<span id=child style='line-height:32px;line-height:{value}'></span>"),
                "child"
            )
            .line_height,
            32.0
        );
    }
}

#[test]
fn computed_cssom_value_distinguishes_normal_from_used_pixels() {
    for (value, expected) in [("normal", "normal"), ("1.5", "24px"), ("150%", "24px")] {
        let style = computed(
            &format!("<span id=child style='font-size:16px;line-height:{value}'></span>"),
            "child",
        );
        assert_eq!(
            resolved_property_value(&style, "line-height").as_deref(),
            Some(expected)
        );
    }
}

#[test]
fn changed_inherited_multiplier_reaches_descendants_with_other_font_sizes() {
    let dom = dom::parse(
        "<div id=parent style='font-size:20px;line-height:30px'><span id=child style='font-size:40px'></span></div>",
    );
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let parent = dom.elements_named("div").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    assert_eq!(styles.get(&child).line_height, 30.0);
    parent.set_attr("style", "font-size:20px;line-height:1.5");
    let stats = styles.refresh_subtrees(&dom.document, &[parent], &[]);
    assert!(stats.layout_changed);
    assert_eq!(styles.get(&child).line_height, 60.0);
}

#[test]
fn line_boxes_allow_negative_leading_without_clipping_glyphs() {
    use crate::engine::layout::{DisplayItem, FontSpec, TextMeasurer, layout_page};
    struct Font;
    impl TextMeasurer for Font {
        fn measure(&mut self, text: &str, font: &FontSpec) -> (f32, f32) {
            (text.len() as f32 * 8.0, font.size)
        }
    }
    for height in [0.0, 8.0, 24.0] {
        let page = crate::engine::Page::parse(
            &format!(
                "<style>body{{margin:0}}p{{margin:0;font:16px/{height}px Arial}}</style><p>a<br>b</p>"
            ),
            "https://example.test/",
        );
        let output = layout_page(&page, 800.0, 600.0, &mut Font);
        let node = page.dom.elements_named("p").next().unwrap();
        assert_eq!(output.node_bounds[&node.id()].height, 2.0 * height);
        let glyphs: Vec<_> = output
            .items
            .iter()
            .filter_map(|item| match item {
                DisplayItem::Text { rect, .. } => Some(*rect),
                _ => None,
            })
            .collect();
        assert_eq!(glyphs.len(), 2);
        assert_eq!(glyphs[0].height, 16.0);
        assert_eq!(glyphs[1].y - glyphs[0].y, height);
    }
}
