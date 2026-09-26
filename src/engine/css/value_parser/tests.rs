use super::*;

#[test]
fn literal_lengths_are_single_css_tokens() {
    for (source, expected) in [
        ("5px", Length::Px(5.0)),
        (" 5PX ", Length::Px(5.0)),
        ("+0", Length::Px(0.0)),
        ("-0.0", Length::Px(0.0)),
        (".5em", Length::Em(0.5)),
        ("1e2px", Length::Px(100.0)),
        ("12.5%", Length::Percent(12.5)),
        ("AUTO", Length::Auto),
    ] {
        assert_eq!(parse_length(source), Some(expected), "{source}");
    }
    for source in [
        "5 px",
        "5\tpx",
        "5 %",
        "5px 1px",
        "NaNpx",
        "infpx",
        "5px!important",
    ] {
        assert_eq!(parse_length(source), None, "{source}");
    }
    assert_eq!(parse_length("calc(5px + 2px)"), Some(Length::Px(7.0)));
    assert_eq!(parse_length("calc(5 px + 2px)"), None);
}

#[test]
fn malformed_dimension_declarations_do_not_override_valid_lengths() {
    let dom = dom::parse(
        r#"<style>
             div { width: 12px; margin-left: 3px; padding-top: 2px }
             #bad { width: 5 px; margin-left: 4 px; padding-top: 2 % }
             #good { width: 5PX; margin-left: .5em; padding-top: 0 }
           </style><div id=bad></div><div id=good></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let bad = dom
        .elements_named("div")
        .find(|node| node.attr_ref("id").as_deref() == Some("bad"))
        .unwrap();
    let good = dom
        .elements_named("div")
        .find(|node| node.attr_ref("id").as_deref() == Some("good"))
        .unwrap();
    assert_eq!(styles.get(&bad).width, Length::Px(12.0));
    assert_eq!(styles.get(&bad).margin.left, Length::Px(3.0));
    assert_eq!(styles.get(&bad).padding.top, Length::Px(2.0));
    assert_eq!(styles.get(&good).width, Length::Px(5.0));
    assert_eq!(styles.get(&good).margin.left, Length::Em(0.5));
    assert_eq!(styles.get(&good).padding.top, Length::Px(0.0));
    assert!(!supports::supports_declaration_value("width", "5 px"));
    assert!(supports::supports_declaration_value("width", "5px"));
}
