use super::*;

#[test]
fn integer_repeat_counts_use_shared_css_math_and_positive_rounding() {
    for (source, count) in [
        ("repeat(3, 10px)", 3),
        ("repeat(calc(1 + 2), 10px)", 3),
        ("repeat(round(2.5), 10px)", 3),
        ("repeat(calc(-3), 10px)", 1),
        ("repeat(pow(2,3), 10px)", 8),
        (r"r\65 peat(calc(6 / 2), 10px)", 3),
        ("REPEAT(/* count */ 2, /* size */ 10PX)", 2),
    ] {
        let tracks = parse(source).unwrap_or_else(|| panic!("{source}"));
        assert_eq!(tracks.len(), count, "{source}");
        assert!(
            tracks
                .iter()
                .all(|track| *track == GridTrack::Fixed(Length::Px(10.0)))
        );
    }
    for source in [
        "repeat(0,10px)",
        "repeat(-1,10px)",
        "repeat(1.5,10px)",
        "repeat(1e0,10px)",
        "repeat(calc(1px),10px)",
        "repeat(sign(1em),10px)",
        "repeat(auto-fit,10px)",
        "repeat(auto-fill,10px)",
        "repeat(1,)",
        "repeat(1,10px,20px)",
        "repeat (2,10px)",
        "repeat/**/(2,10px)",
    ] {
        assert!(parse(source).is_none(), "{source}");
    }
}

#[test]
fn repetition_limits_the_total_expansion_and_never_nests() {
    for source in [
        "repeat(2147483647,1px)",
        "repeat(calc(infinity),1px 2px)",
        "repeat(8000,1px) repeat(8000,2px)",
        "1px repeat(10000,2px 3px) 4px",
    ] {
        assert_eq!(parse(source).unwrap().len(), MAX_TRACKS, "{source}");
    }
    let tracks = parse("1px repeat(10000,2px 3px) 4px").unwrap();
    assert_eq!(tracks[0], GridTrack::Fixed(Length::Px(1.0)));
    assert_eq!(tracks[1], GridTrack::Fixed(Length::Px(2.0)));
    assert_eq!(tracks[2], GridTrack::Fixed(Length::Px(3.0)));
    for source in [
        "repeat(2,repeat(2,1px))",
        "minmax(1px,minmax(2px,3px))",
        "repeat(10000,1px) nonsense",
        "repeat(10000,1px) -1fr",
        "repeat(2,[name])",
    ] {
        assert!(parse(source).is_none(), "{source}");
    }
    let nested = format!("{}1px{}", "repeat(2,".repeat(100), ")".repeat(100));
    assert!(parse(&nested).is_none());
    assert!(parse(&format!("1px /*{}*/", "x".repeat(16_384))).is_none());
}

#[test]
fn fractions_are_tokens_not_defaulted_rust_number_strings() {
    for (source, expected) in [("0fr", 0.0), (".5fr", 0.5), ("+2FR", 2.0), ("2e1fr", 20.0)] {
        assert_eq!(
            parse(source),
            Some(vec![GridTrack::Fraction(expected)]),
            "{source}"
        );
    }
    for source in [
        "fr", "NaNfr", "inffr", "-1fr", "1 fr", "2fr junk", "1fr/2fr",
    ] {
        assert!(parse(source).is_none(), "{source}");
    }
}

#[test]
fn track_breadths_preserve_math_dependencies_and_minimum_grammar() {
    let tracks = parse("minmax(max(1em,10%),1fr) hypot(3px,4px)").unwrap();
    let GridTrack::MinMax(minimum, maximum) = &tracks[0] else {
        panic!("minmax");
    };
    assert_eq!(maximum.as_ref(), &GridTrack::Fraction(1.0));
    let GridTrack::Fixed(length) = minimum.as_ref() else {
        panic!("minimum");
    };
    assert_eq!(length.resolve(100.0, 16.0), Some(16.0));
    assert_eq!(length.resolve(300.0, 16.0), Some(30.0));
    assert_eq!(length.resolve(100.0, 40.0), Some(40.0));
    let GridTrack::Fixed(length) = &tracks[1] else {
        panic!("hypot");
    };
    assert_eq!(length.resolve(0.0, 16.0), Some(5.0));
    assert!(parse("calc(-1px)").is_some());
    for source in [
        "minmax(1fr,2fr)",
        "minmax(10px)",
        "minmax(10px,20px,30px)",
        "minmax(-1px,1fr)",
        "minmax(10px,repeat(2,1fr))",
        "minmax(10px,nonsense)",
        "-1px",
        "-1%",
        "-1em",
        "10px nonsense 20px",
        "fit-content(30px)",
        "calc(1)",
        "calc(1deg)",
        "calc(1s)",
        "calc(1fr)",
    ] {
        assert!(parse(source).is_none(), "{source}");
    }
}

#[test]
fn line_name_and_area_tokens_are_not_arbitrary_discarded_garbage() {
    assert_eq!(
        parse("[start] 10px [end]"),
        Some(vec![GridTrack::Fixed(Length::Px(10.0))])
    );
    assert_eq!(parse("repeat(2,[start]10px[end])").unwrap().len(), 2);
    assert!(parse("[auto]10px").is_none());
    assert!(parse("[span]10px").is_none());
    assert!(parse("[1]10px").is_none());
    assert!(parse("'header' 10px").is_none());
    assert!(layout_listing("'header'10px 'body'1fr").is_some());
    assert_eq!(parse("none"), Some(vec![]));
    assert!(parse("none 10px").is_none());
}

#[test]
fn cssom_and_capabilities_share_the_implemented_track_contract() {
    for source in [
        "repeat(calc(2.5),10px)",
        "max(1em,10%) 1fr",
        "minmax(0,1fr)",
        "none",
    ] {
        assert!(
            supports::supports_declaration_value("grid-template-columns", source),
            "{source}"
        );
        assert!(
            supports::supports_declaration_value("grid-template-rows", source),
            "{source}"
        );
    }
    for source in [
        "repeat(auto-fit,10px)",
        "fit-content(10px)",
        "[start]10px",
        "NaNfr",
        "minmax(1fr,2fr)",
    ] {
        assert!(
            !supports::supports_declaration_value("grid-template-columns", source),
            "{source}"
        );
    }
    // Author token storage is still retained for computed-value serialization.
    // An invalid replacement must not erase a previously admitted longhand.
    for name in ["grid-template-columns", "grid-template-rows"] {
        let mut style = ComputedStyle::initial();
        let initial = ComputedStyle::initial();
        let context = || DeclarationContext {
            parent: None,
            lower_origin: &initial,
            layer_start: &initial,
            base_url: "https://example.test/",
            viewport_width: 800.0,
            viewport_height: 600.0,
        };
        apply_declaration(&mut style, (name, "repeat(calc(1 + 2),10px)"), context());
        apply_declaration(&mut style, (name, "minmax(1fr,2fr)"), context());
        let stored = if name.ends_with("columns") {
            &style.grid_template_columns
        } else {
            &style.grid_template_rows
        };
        assert_eq!(stored, "repeat(calc(1 + 2),10px)");
    }
}
