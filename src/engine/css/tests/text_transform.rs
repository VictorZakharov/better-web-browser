use super::*;

#[test]
fn text_transform_parses_only_the_supported_css_text_keywords() {
    for (input, expected) in [
        ("none", TextTransform::None),
        ("capitalize", TextTransform::Capitalize),
        ("uppercase", TextTransform::Uppercase),
        ("lowercase", TextTransform::Lowercase),
    ] {
        assert_eq!(TextTransform::parse(input), Some(expected));
        assert_eq!(TextTransform::parse(input).unwrap().css_text(), input);
        assert!(supports::supports_matches(&format!(
            "@supports (text-transform: {input})"
        )));
    }
    for unsupported in [
        "",
        "full-width",
        "full-size-kana",
        "math-auto",
        "uppercase lowercase",
        "capitalize junk",
    ] {
        assert_eq!(TextTransform::parse(unsupported), None, "{unsupported}");
        assert!(
            !supports::supports_matches(&format!("@supports (text-transform: {unsupported})")),
            "{unsupported}"
        );
    }
    // CSS identifiers are ASCII case-insensitive, including in @supports.
    assert_eq!(
        TextTransform::parse("UpPeRcAsE"),
        Some(TextTransform::Uppercase)
    );
    assert!(supports::supports_matches(
        "@supports (TEXT-TRANSFORM: UpPeRcAsE)"
    ));
}

#[test]
fn text_transform_inherits_and_css_wide_keywords_restore_the_right_value() {
    let dom = dom::parse(
        "<style>div{text-transform:uppercase}#explicit{text-transform:lowercase}\
         #initial{text-transform:initial}#unset{text-transform:unset}\
         #inherited{text-transform:inherit}#invalid{text-transform:full-width}</style>\
         <div><span id=plain></span><span id=explicit></span>\
         <span id=initial></span><span id=unset></span>\
         <span id=inherited></span><span id=invalid></span></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let get = |id| {
        let node = dom
            .elements_named("span")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap();
        styles.get(&node).text_transform
    };
    assert_eq!(get("plain"), TextTransform::Uppercase);
    assert_eq!(get("explicit"), TextTransform::Lowercase);
    assert_eq!(get("initial"), TextTransform::None);
    assert_eq!(get("unset"), TextTransform::Uppercase);
    assert_eq!(get("inherited"), TextTransform::Uppercase);
    assert_eq!(get("invalid"), TextTransform::Uppercase);
}

#[test]
fn computed_style_serializes_the_cascaded_value() {
    let dom = dom::parse("<style>p{text-transform:capitalize}</style><p>first word</p>");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let style = styles.get(&dom.elements_named("p").next().unwrap());
    assert_eq!(
        resolved_property_value(style, "text-transform").as_deref(),
        Some("capitalize")
    );
}
