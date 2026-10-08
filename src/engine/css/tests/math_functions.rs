use super::super::value_parser::parse_opacity;
use super::*;

#[test]
fn opacity_uses_css_tokens_instead_of_rust_numeric_spellings() {
    for (source, expected) in [
        ("25%", 0.25),
        ("25%/**/", 0.25),
        ("+.5", 0.5),
        (".5/**/", 0.5),
        ("1e-1", 0.1),
        ("calc(50% / 2)", 0.25),
    ] {
        assert_eq!(parse_opacity(source), Some(expected), "{source}");
    }
    for source in ["25 %", "25/**/%", "NaN", "inf", "0x1", "+ .5", ".5 junk"] {
        assert_eq!(parse_opacity(source), None, "{source}");
        for markup in [
            format!("<div style='opacity:.7;opacity:{source}'></div>"),
            format!("<style>div{{opacity:.7;opacity:{source}}}</style><div></div>"),
        ] {
            let dom = dom::parse(&markup);
            let styles = StyleSet::from_dom(&dom, &[], 800.0);
            let node = dom.elements_named("div").next().unwrap();
            assert_eq!(styles.get(&node).opacity, 0.7, "{source}");
        }
    }
}

#[test]
fn scalar_math_opacity_and_unitless_line_height_keep_their_property_types() {
    let dom = dom::parse(
        r#"<style>
        #parent{font-size:20px;line-height:calc(sqrt(4));opacity:calc(log(8,2) / 4)}
        #child{font-size:10px}
        #percentage{opacity:round(55%,10%)}
        #invalid{opacity:.7;opacity:min(20px,40px);opacity:calc(25% + .25)}
        </style><div id=parent><span id=child>x</span></div><div id=percentage></div><div id=invalid></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let parent = dom.elements_named("div").next().unwrap();
    let child = dom.elements_named("span").next().unwrap();
    assert_eq!(styles.get(&parent).line_height, 40.0);
    assert_eq!(styles.get(&child).line_height, 20.0);
    assert_eq!(styles.get(&parent).opacity, 0.75);
    assert_eq!(
        styles
            .get(&dom.elements_named("div").nth(1).unwrap())
            .opacity,
        0.6
    );
    assert_eq!(
        styles
            .get(&dom.elements_named("div").nth(2).unwrap())
            .opacity,
        0.7
    );
    for value in [
        "calc(.5 + .25)",
        "round(55%,10%)",
        "sqrt(.25)",
        "calc(sin(90deg) / 2)",
    ] {
        assert!(
            supports::supports_declaration_value("opacity", value),
            "{value}"
        );
    }
    for value in ["calc(25% + .25)", "min(20px,40px)", "calc(2em)"] {
        assert!(
            !supports::supports_declaration_value("opacity", value),
            "{value}"
        );
    }
}

#[test]
fn scalar_and_stepped_ranges_clamp_only_after_computation() {
    let dom = dom::parse(
        r#"<div style="opacity:calc(-2);line-height:calc(-2);border-top-width:round(down,-4px,3px);border-right-width:calc(sqrt(-1) * 1px)"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    assert_eq!(style.opacity, 0.0);
    assert_eq!(style.line_height, 0.0);
    assert_eq!(style.border_width.top, Length::Px(0.0));
    assert_eq!(style.border_width.right, Length::Px(0.0));
    assert!(supports::supports_declaration_value(
        "width",
        "calc(1px / 0)"
    ));
}

#[test]
fn computed_serialization_retains_deferred_functions_after_font_normalization() {
    let dom = dom::parse(
        r#"<style>html{font-size:10px}</style><div style="font-size:20px;margin-left:round(up,50%,2em);width:calc(sign(50% - 20rem) * 100px)"></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let node = dom.elements_named("div").next().unwrap();
    let style = styles.get(&node);
    for property in ["margin-left", "width"] {
        let text = cssom::resolved_property_value(style, property).unwrap();
        assert!(!text.contains("em"), "{text}");
        let value = parse_length(&text).unwrap();
        let original = if property == "width" {
            &style.width
        } else {
            &style.margin.left
        };
        for basis in [0.0, 100.0, 500.0, 1000.0] {
            assert_eq!(
                value.resolve(basis, 99.0),
                original.resolve(basis, 20.0),
                "{text}"
            );
        }
    }
}

#[test]
fn scalar_math_flows_through_shorthands_cascade_and_supports() {
    let dom = dom::parse(
        r#"<style>
        #factors{flex:calc(1 + 2) sqrt(4) round(25px,10px);z-index:calc(-1.5);aspect-ratio:calc(16 / 2) / sqrt(4)}
        #invalid{flex:2 3 20px;flex:2 20px 3;flex-grow:-1;flex-shrink:NaN;z-index:4;z-index:1.5;aspect-ratio:3 / 2;aspect-ratio:calc(16px) / 9}
        #omitted{flex:2}
        #explicit{flex:2 1 0}
        </style><div id=factors></div><div id=invalid></div><div id=omitted></div><div id=explicit></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let nodes: Vec<_> = dom.elements_named("div").collect();
    let factors = styles.get(&nodes[0]);
    assert_eq!(factors.flex_grow, 3.0);
    assert_eq!(factors.flex_shrink, 2.0);
    assert_eq!(factors.flex_basis.resolve(100.0, 16.0), Some(30.0));
    assert_eq!(factors.z_index, Some(-1));
    assert_eq!(factors.aspect_ratio.preferred(None), Some((4.0, true)));
    let invalid = styles.get(&nodes[1]);
    assert_eq!((invalid.flex_grow, invalid.flex_shrink), (2.0, 3.0));
    assert_eq!(invalid.flex_basis, Length::Px(20.0));
    assert_eq!(invalid.z_index, Some(4));
    assert_eq!(invalid.aspect_ratio.preferred(None), Some((1.5, true)));
    assert_eq!(styles.get(&nodes[2]).flex_basis, Length::Percent(0.0));
    assert_eq!(styles.get(&nodes[3]).flex_basis, Length::Px(0.0));
    for (property, value, expected) in [
        ("flex", "calc(1 + 2) sqrt(4) round(25px,10px)", true),
        ("flex", "round(25px,10px) calc(1 + 2) sqrt(4)", true),
        ("flex", "2 20px 3", false),
        ("flex-grow", "sqrt(4)", true),
        ("flex-grow", "-1", false),
        ("flex-shrink", "calc(-1)", true),
        ("flex-shrink", "NaN", false),
        ("z-index", "calc(-1.5)", true),
        ("z-index", "1.5", false),
        ("aspect-ratio", "calc(16 / 2) / sqrt(4)", true),
        ("aspect-ratio", "16px / 9", false),
    ] {
        assert_eq!(
            supports::supports_declaration_value(property, value),
            expected,
            "{property}:{value}"
        );
    }
}
