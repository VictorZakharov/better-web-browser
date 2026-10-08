//! Comparison functions must use the same grammar in cascade, CSS.supports,
//! computed serialization and layout; accepting a keyword is not implementation.
use super::*;

#[test]
fn computed_math_preserves_percentage_branches_until_layout() {
    let dom = dom::parse(
        r#"<style>html{font-size:10px}#target{font-size:20px;width:min(50%, 20rem);margin-left:max(10%, 2em)}</style><div id=target></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    assert_eq!(style.width.resolve(300.0, style.font_size), Some(150.0));
    assert_eq!(style.width.resolve(600.0, style.font_size), Some(200.0));
    assert_eq!(
        style.margin.left.resolve(300.0, style.font_size),
        Some(40.0)
    );
    assert_eq!(
        style.margin.left.resolve(600.0, style.font_size),
        Some(60.0)
    );
    let width = cssom::resolved_property_value(style, "width").unwrap();
    assert_eq!(
        parse_length(&width).unwrap().resolve(300.0, 20.0),
        Some(150.0)
    );
    assert_eq!(
        parse_length(&width).unwrap().resolve(600.0, 20.0),
        Some(200.0)
    );
    let margin = cssom::resolved_property_value(style, "margin-left").unwrap();
    assert!(
        !margin.contains("em"),
        "computed em must serialize in pixels: {margin}"
    );
    assert_eq!(
        parse_length(&margin).unwrap().resolve(300.0, 99.0),
        Some(40.0)
    );
}

#[test]
fn math_font_size_and_line_height_use_their_own_percentage_basis() {
    let dom = dom::parse(
        r#"<style>body{font-size:20px}#parent{font-size:clamp(10px, 150%, 40px);line-height:max(1.5em, 20px)}#child{font-size:10px}</style><div id=parent><span id=child>x</span></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let parent = dom.elements_named("div").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    assert_eq!(styles.get(&parent).font_size, 30.0);
    assert_eq!(styles.get(&parent).line_height, 45.0);
    // A computed length line-height is inherited as pixels, not recalculated
    // using the child's smaller font size (CSS 2.2 §10.8.1).
    assert_eq!(styles.get(&child).line_height, 45.0);
}

#[test]
fn math_declarations_and_supports_share_typed_grammar() {
    for (property, value) in [
        ("width", "min(620px, 88vw)"),
        ("max-height", "max(20px, 10vh)"),
        ("margin", "min(1em, 10px) clamp(2px, 5%, 20px)"),
        ("padding", "min(10px, 2vw)"),
        ("font-size", "clamp(10px, 150%, 40px)"),
        ("line-height", "max(1.5em, 20px)"),
        ("scroll-padding", "min(50%, 20px)"),
    ] {
        assert!(
            supports::supports_declaration_value(property, value),
            "{property}: {value}"
        );
    }
    for value in [
        "min(1, 20px)",
        "max(1deg, 20px)",
        "clamp(10px, 2em)",
        "min(20px,)",
        "calc(1px+2px)",
    ] {
        assert!(
            !supports::supports_declaration_value("width", value),
            "{value}"
        );
    }
}

#[test]
fn malformed_math_does_not_replace_a_valid_earlier_declaration() {
    let dom = dom::parse(
        r#"<div style="width:80px;width:min(1,20px);margin:4px;margin:min(20px,10%) unknown;padding:3px;padding:clamp(1px, 2px);background-size:15px 20px;background-size:min(2px,3px) junk"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    assert_eq!(style.width, Length::Px(80.0));
    assert_eq!(style.margin, uniform_edges(Length::Px(4.0)));
    assert_eq!(style.padding, uniform_edges(Length::Px(3.0)));
    assert_eq!(
        style.background_size,
        BackgroundSize::Explicit {
            width: Length::Px(15.0),
            height: Length::Px(20.0)
        }
    );
}

#[test]
fn shorthand_comparison_functions_keep_each_axis_independent() {
    let dom = dom::parse(
        r#"<div style="gap:min(20px, 10%) max(4px, 1em);background-size:min(50%, 200px) clamp(10px, 3em, 80px)"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    assert_eq!(style.grid_row_gap.resolve(100.0, 16.0), Some(10.0));
    assert_eq!(style.grid_column_gap.resolve(100.0, 16.0), Some(16.0));
    let BackgroundSize::Explicit { width, height } = &style.background_size else {
        panic!("size rejected");
    };
    assert_eq!(width.resolve(300.0, 16.0), Some(150.0));
    assert_eq!(height.resolve(300.0, 16.0), Some(48.0));
}

#[test]
fn shorthand_tokenization_ignores_comment_parentheses_and_accepts_css_escapes() {
    let dom = dom::parse(
        r#"<div style="margin:min(10px, /* ) */ 20px)/**/max(3px,4px);padding:m\69 n(1px, 2px) 3px"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    assert_eq!(style.margin.top.resolve(100.0, 16.0), Some(10.0));
    assert_eq!(style.margin.left.resolve(100.0, 16.0), Some(4.0));
    assert_eq!(style.padding.top.resolve(100.0, 16.0), Some(1.0));
    assert_eq!(style.padding.left.resolve(100.0, 16.0), Some(3.0));
}

#[test]
fn comparison_border_widths_enforce_length_only_grammar_and_computed_range() {
    let dom = dom::parse(
        r#"<div id=range style="border-width:4px;border-top-width:calc(1px - 2px);border-right-width:clamp(-3px,-2px,-1px);border-bottom-width:thin;border-left-width:max(1px,3px)"></div><div id=invalid style="border-width:3px;border-width:min(0%);border-left-width:calc(10% - 10%)"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let range = dom.elements_named("div").next().unwrap();
    let invalid = dom.elements_named("div").nth(1).unwrap();
    let range = styles.get(&range);
    assert_eq!(range.border_width.top, Length::Px(0.0));
    assert_eq!(range.border_width.right, Length::Px(0.0));
    assert_eq!(range.border_width.bottom, Length::Px(1.0));
    assert_eq!(range.border_width.left, Length::Px(3.0));
    assert_eq!(
        styles.get(&invalid).border_width,
        uniform_edges(Length::Px(3.0))
    );
    assert!(supports::supports_declaration_value(
        "border-width",
        "min(1px,2px) thick"
    ));
    assert!(!supports::supports_declaration_value(
        "border-left-width",
        "min(1px,0%)"
    ));
}
