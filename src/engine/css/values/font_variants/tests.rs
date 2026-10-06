use super::*;

#[test]
fn ligature_groups_are_independent_case_insensitive_and_canonically_ordered() {
    let parsed =
        FontLigatures::parse("no-contextual HISTORICAL-LIGATURES no-common-ligatures").unwrap();
    assert_eq!(
        parsed.css_text(),
        "no-common-ligatures historical-ligatures no-contextual"
    );
    let settings = FontVariants::new(parsed, FontNumeric::default()).settings();
    assert_eq!(
        settings,
        vec![(*b"liga", 0), (*b"clig", 0), (*b"hlig", 1), (*b"calt", 0)]
    );
    assert_eq!(FontLigatures::parse("none").unwrap().css_text(), "none");
    let explicit = FontLigatures::parse(
        "no-common-ligatures no-discretionary-ligatures no-historical-ligatures no-contextual",
    )
    .unwrap();
    assert_eq!(
        explicit.css_text(),
        "no-common-ligatures no-discretionary-ligatures no-historical-ligatures no-contextual"
    );
    assert_eq!(
        FontVariants::new(explicit, FontNumeric::default()).bits(),
        FontVariants::new(
            FontLigatures::parse("none").unwrap(),
            FontNumeric::default()
        )
        .bits()
    );
    assert_eq!(
        FontLigatures::parse(r"c\6f mmon-ligatures")
            .unwrap()
            .css_text(),
        "common-ligatures"
    );
}

#[test]
fn ligature_duplicate_conflicts_and_unknown_components_reject_the_whole_value() {
    for value in [
        "",
        "normal contextual",
        "none common-ligatures",
        "normal none",
        "common-ligatures no-common-ligatures",
        "contextual contextual",
        "liga",
        "1",
        "common-ligatures, contextual",
        "common-ligatures bogus",
    ] {
        assert!(FontLigatures::parse(value).is_none(), "{value}");
    }
    for options in LIGATURE_NAMES {
        for first in options {
            for second in options {
                assert!(FontLigatures::parse(&format!("{first} {second}")).is_none());
            }
        }
    }
}

#[test]
fn numeric_features_enable_the_chosen_feature_and_disable_its_conflicting_default() {
    let numeric =
        FontNumeric::parse("slashed-zero ordinal stacked-fractions tabular-nums oldstyle-nums")
            .unwrap();
    assert_eq!(
        numeric.css_text(),
        "oldstyle-nums tabular-nums stacked-fractions ordinal slashed-zero"
    );
    assert_eq!(
        FontVariants::new(FontLigatures::default(), numeric).settings(),
        vec![
            (*b"lnum", 0),
            (*b"onum", 1),
            (*b"pnum", 0),
            (*b"tnum", 1),
            (*b"frac", 0),
            (*b"afrc", 1),
            (*b"ordn", 1),
            (*b"zero", 1)
        ]
    );
    let opposite = FontNumeric::parse("lining-nums proportional-nums diagonal-fractions").unwrap();
    assert_eq!(
        FontVariants::new(FontLigatures::default(), opposite).settings(),
        vec![
            (*b"lnum", 1),
            (*b"onum", 0),
            (*b"pnum", 1),
            (*b"tnum", 0),
            (*b"frac", 1),
            (*b"afrc", 0)
        ]
    );
}

#[test]
fn numeric_values_reject_conflicts_duplicates_unknown_tokens_and_css_wide_keywords() {
    for value in [
        "",
        "none",
        "lining-nums oldstyle-nums",
        "tabular-nums proportional-nums",
        "diagonal-fractions stacked-fractions",
        "ordinal ordinal",
        "slashed-zero slashed-zero",
        "normal ordinal",
        "inherit",
        "oldstyle-nums invalid",
        "ordinal, slashed-zero",
    ] {
        assert!(FontNumeric::parse(value).is_none(), "{value}");
    }
    assert_eq!(
        FontNumeric::parse("NORMAL /* comment */")
            .unwrap()
            .css_text(),
        "normal"
    );
}

#[test]
fn supported_variant_shorthand_resets_both_groups_and_rejects_unimplemented_caps() {
    let (ligatures, numeric) =
        FontVariants::parse_shorthand("ordinal no-common-ligatures tabular-nums").unwrap();
    assert_eq!(ligatures.css_text(), "no-common-ligatures");
    assert_eq!(numeric.css_text(), "tabular-nums ordinal");
    let (ligatures, numeric) = FontVariants::parse_shorthand("normal").unwrap();
    assert_eq!(
        (ligatures.css_text(), numeric.css_text()),
        ("normal".into(), "normal".into())
    );
    let (ligatures, numeric) = FontVariants::parse_shorthand("none").unwrap();
    assert_eq!(
        (ligatures.css_text(), numeric.css_text()),
        ("none".into(), "normal".into())
    );
    for value in [
        "small-caps",
        "normal ordinal",
        "common-ligatures no-common-ligatures",
        "ordinal ordinal",
    ] {
        assert!(FontVariants::parse_shorthand(value).is_none(), "{value}");
    }
}

#[test]
fn every_valid_wire_encoding_round_trips_and_invalid_conflict_pairs_are_rejected() {
    let mut valid = 0;
    for bits in 0..=u16::MAX {
        let expected = (0..7).all(|index| ((bits >> (index * 2)) & 3) != 3);
        assert_eq!(
            FontVariants::from_bits(bits).is_some(),
            expected,
            "{bits:#x}"
        );
        if let Some(variants) = FontVariants::from_bits(bits) {
            assert_eq!(variants.bits(), bits);
            valid += 1;
            assert!(variants.settings().len() <= 13);
        }
    }
    assert_eq!(valid, 3usize.pow(7) * 4);
}
