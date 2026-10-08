use super::*;

#[test]
fn mathematical_ratio_sides_do_not_split_nested_division() {
    for (source, width, height, prefer_natural) in [
        ("16 / 9", 16.0, 9.0, false),
        ("auto 16/9", 16.0, 9.0, true),
        ("16/9 AUTO", 16.0, 9.0, true),
        ("calc(16 / 2) / sqrt(4)", 8.0, 2.0, false),
        ("auto pow(2,3) / calc(1 + 1)", 8.0, 2.0, true),
        ("calc(16 / 9)", 16.0 / 9.0, 1.0, false),
        ("calc(-2) / 1", 0.0, 1.0, false),
        ("calc(NaN) / 1", 0.0, 1.0, false),
        ("16/**/ / /**/9", 16.0, 9.0, false),
    ] {
        assert_eq!(
            AspectRatio::parse(source),
            Some(AspectRatio::Ratio {
                width,
                height,
                prefer_natural
            }),
            "{source}"
        );
    }
}

#[test]
fn invalid_ratio_arity_and_dimensions_are_rejected() {
    for source in [
        "16 9",
        "16//9",
        "16 / 9 / 2",
        "auto auto",
        "16 auto 9",
        "auto 16 / 9 auto",
        "-2 / 1",
        "16px / 9",
        "16 / 9%",
        "calc(16px) / 9",
        "calc(16% + 2) / 9",
        "calc(16 /) / 9",
        "16 / sqrt(4px)",
        "NaN / 1",
        "inf / 1",
    ] {
        assert_eq!(AspectRatio::parse(source), None, "{source}");
    }
}

#[test]
fn degenerate_and_natural_preferences_remain_separate_from_number_parsing() {
    let ratio = AspectRatio::parse("auto calc(16 / 2) / sqrt(4)").unwrap();
    assert_eq!(ratio.preferred(Some((300.0, 200.0))), Some((1.5, false)));
    assert_eq!(ratio.preferred(None), Some((4.0, false)));
    assert_eq!(
        AspectRatio::parse("calc(-1) / 2").unwrap().preferred(None),
        None
    );
    assert_eq!(
        AspectRatio::parse("calc(-1) / 2")
            .unwrap()
            .preferred(Some((300.0, 200.0))),
        Some((1.5, false))
    );
    let text = ratio.css_text();
    assert_eq!(AspectRatio::parse(&text), Some(ratio));
}
