use super::*;

#[test]
fn object_fit_and_position_cascade_and_serialize() {
    let dom = dom::parse(
        "<style>img{object-fit:cover;object-position:right 10px bottom 20%}\
         #bad{object-fit:contain;object-fit:stretch;object-position:left;object-position:bogus}\
         #reset{object-fit:contain;object-fit:initial;object-position:left;object-position:unset}\
         #inherit{object-fit:inherit;object-position:inherit}</style>\
         <div style='object-fit:none;object-position:top'><img id=good><img id=bad>\
         <img id=reset><img id=inherit></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let image = |id| {
        dom.elements_named("img")
            .find(|node| node.attr("id").as_deref() == Some(id))
            .unwrap()
    };
    let good = styles.get(&image("good"));
    assert_eq!(good.object_fit, ObjectFit::Cover);
    assert_eq!(
        good.object_position.css_text(good.font_size),
        "right 10px bottom 20%"
    );
    assert_eq!(
        resolved_property_value(good, "object-fit").as_deref(),
        Some("cover")
    );
    assert_eq!(
        resolved_property_value(good, "object-position").as_deref(),
        Some("right 10px bottom 20%")
    );
    let bad = styles.get(&image("bad"));
    assert_eq!(bad.object_fit, ObjectFit::Contain);
    assert_eq!(bad.object_position.css_text(bad.font_size), "0% 50%");
    let reset = styles.get(&image("reset"));
    assert_eq!(reset.object_fit, ObjectFit::Fill);
    assert_eq!(reset.object_position, ObjectPosition::default());
    let inherit = styles.get(&image("inherit"));
    assert_eq!(inherit.object_fit, ObjectFit::None);
    assert_eq!(
        inherit.object_position.css_text(inherit.font_size),
        "50% 0%"
    );
}

#[test]
fn position_grammar_handles_axes_offsets_and_functions_without_accepting_junk() {
    let parsed = |value: &str| ObjectPosition::parse(value).map(|position| position.css_text(16.0));
    assert_eq!(parsed("top right"), Some("100% 0%".into()));
    assert_eq!(parsed("25% bottom"), Some("25% 100%".into()));
    assert_eq!(parsed("center"), Some("50% 50%".into()));
    assert_eq!(
        parsed("right 10px bottom 20%"),
        Some("right 10px bottom 20%".into())
    );
    assert_eq!(
        parsed("top 1em right 10px"),
        Some("right 10px top 16px".into())
    );
    assert_eq!(
        parsed("calc(50% + 10px) 25%"),
        Some("calc(10px + 50%) 25%".into())
    );
    for invalid in [
        "",
        "foo",
        "left right",
        "top bottom",
        "left top junk",
        "1px 2px 3px",
        "left,,top",
    ] {
        assert!(parsed(invalid).is_none(), "unexpectedly accepted {invalid}");
    }
    assert!(supports::supports_matches(
        "@supports (object-fit: contain)"
    ));
    assert!(supports::supports_matches(
        "@supports (object-position: right 10px bottom 20%)"
    ));
    assert!(!supports::supports_matches(
        "@supports (object-fit: stretch)"
    ));
}
