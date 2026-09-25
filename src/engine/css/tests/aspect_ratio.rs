use super::*;

#[test]
fn preferred_ratio_grammar_and_computed_serialization() {
    for (input, expected) in [
        ("auto", "auto"),
        ("1", "1 / 1"),
        ("16/9", "16 / 9"),
        ("auto 4/3", "auto 4 / 3"),
        ("4 / 3 auto", "auto 4 / 3"),
        ("0 / 1", "0 / 1"),
    ] {
        assert_eq!(
            AspectRatio::parse(input).map(AspectRatio::css_text),
            Some(expected.to_string()),
            "{input}"
        );
    }
    for invalid in [
        "",
        "auto auto",
        "auto 1 auto",
        "1 /",
        "/ 2",
        "1 / 2 / 3",
        "-1 / 2",
        "1 / -2",
        "NaN",
        "inf",
        "1 2",
        "1 / 2 junk",
    ] {
        assert!(AspectRatio::parse(invalid).is_none(), "{invalid}");
    }
    assert!(supports::supports_matches(
        "@supports (aspect-ratio: auto 16 / 9)"
    ));
    assert!(!supports::supports_matches(
        "@supports (aspect-ratio: 1 / junk)"
    ));
}

#[test]
fn preferred_ratio_cascades_without_inheritance_and_obeys_css_wide_keywords() {
    let dom = dom::parse(
        "<style>div{aspect-ratio:4/3}#bad{aspect-ratio:1/1;aspect-ratio:bogus}\
         #reset{aspect-ratio:1/1;aspect-ratio:initial}\
         #inherited{aspect-ratio:inherit}</style>\
         <div><span id=plain></span><span id=inherited></span>\
         <div id=bad></div><div id=reset></div></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let find = |id| {
        dom::Node::descendants(&dom.document)
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let plain = styles.get(&find("plain"));
    assert_eq!(plain.aspect_ratio, AspectRatio::Auto);
    let inherited = styles.get(&find("inherited"));
    assert_eq!(inherited.aspect_ratio.css_text(), "4 / 3");
    let bad = styles.get(&find("bad"));
    assert_eq!(bad.aspect_ratio.css_text(), "1 / 1");
    let reset = styles.get(&find("reset"));
    assert_eq!(
        resolved_property_value(reset, "aspect-ratio").as_deref(),
        Some("auto")
    );
}

#[test]
fn auto_plus_ratio_prefers_a_replaced_elements_natural_ratio() {
    let natural = Some((300.0, 150.0));
    assert_eq!(AspectRatio::Auto.preferred(natural), Some((2.0, false)));
    assert_eq!(
        AspectRatio::parse("1/1").unwrap().preferred(natural),
        Some((1.0, true))
    );
    assert_eq!(
        AspectRatio::parse("auto 1/1").unwrap().preferred(natural),
        Some((2.0, false))
    );
    assert_eq!(
        AspectRatio::parse("auto 1/1").unwrap().preferred(None),
        Some((1.0, false))
    );
    assert_eq!(
        AspectRatio::parse("0/1").unwrap().preferred(natural),
        Some((2.0, false))
    );
}
