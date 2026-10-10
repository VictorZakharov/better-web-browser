use super::*;
fn style(css: &str) -> ComputedStyle {
    let page = crate::engine::Page::parse(
        &format!("<style>i{{{css}}}</style><i></i>"),
        "https://example.test/css/site.css",
    );
    page.style(800.0)
        .get(&page.dom.elements_named("i").next().unwrap())
        .clone()
}
#[test]
fn mask_shorthand_resets_geometry_without_accepting_background_colors() {
    let s = style(
        "mask:url('a.svg') center / contain no-repeat;mask: red;mask:linear-gradient(black,transparent) bogus",
    );
    assert_eq!(
        s.mask_image.as_deref(),
        Some("https://example.test/css/a.svg")
    );
    assert_eq!(
        s.mask_layers.at(0),
        (
            BackgroundSize::Contain,
            Length::Percent(50.0),
            Length::Percent(50.0),
            (false, false)
        )
    );
    assert!(s.background_image.is_none());
    let s = style("mask-size:contain;mask-position:center;mask-repeat:no-repeat;mask:none");
    assert!(s.mask_image.0.is_empty());
    assert_eq!(s.mask_layers, BackgroundLayers::default());
}
#[test]
fn supported_mask_queries_use_the_same_strict_parser() {
    for (name, value) in [
        (
            "mask-image",
            "linear-gradient(#000 calc(100% - 18px),#0000)",
        ),
        ("-webkit-mask", "url(a.svg) center / contain no-repeat"),
        ("mask-repeat", "repeat-x, no-repeat"),
    ] {
        assert!(
            super::super::supports::supports_declaration_value(name, value),
            "{name}:{value}"
        );
    }
    for (name, value) in [
        ("mask", "red"),
        ("mask-image", "url(a.svg) garbage"),
        ("mask", "linear-gradient(black,black) content-box exclude"),
        ("mask-size", "contain contain"),
    ] {
        assert!(
            !super::super::supports::supports_declaration_value(name, value),
            "{name}:{value}"
        );
    }
}
#[test]
fn multiple_none_is_preserved_and_prefixed_longhands_serialize() {
    let s = style(
        "-webkit-mask-image:none, none;-webkit-mask-position:10px 20px,center;-webkit-mask-size:contain;-webkit-mask-repeat:no-repeat",
    );
    assert_eq!(s.mask_image.0.len(), 2);
    assert_eq!(
        resolved_property_value(&s, "mask-image").as_deref(),
        Some("none, none")
    );
    assert_eq!(
        resolved_property_value(&s, "-webkit-mask-position").as_deref(),
        Some("10px 20px, 50% 50%")
    );
    assert_eq!(
        resolved_property_value(&s, "mask-size").as_deref(),
        Some("contain")
    );
    let reset = style("mask-image:linear-gradient(black,transparent);mask-image:initial");
    assert!(reset.mask_image.0.is_empty());
}
