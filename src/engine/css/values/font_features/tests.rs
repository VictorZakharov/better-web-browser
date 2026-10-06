use super::*;

#[test]
fn features_are_ascii_case_sensitive_tags_with_last_value_and_sorted_computed_order() {
    let features = FontFeatures::parse(r#"'liga' off, "kern" on, 'liga' 3, 'KERN'"#).unwrap();
    assert_eq!(
        features.settings(),
        &[(*b"KERN", 1), (*b"kern", 1), (*b"liga", 3)]
    );
    assert_eq!(features.css_text(), r#""KERN", "kern", "liga" 3"#);
    assert_eq!(
        FontFeatures::parse(r"'k\65rn' /* comment */ off")
            .unwrap()
            .settings(),
        &[(*b"kern", 0)]
    );
    assert_eq!(
        FontFeatures::parse("NORMAL").unwrap(),
        FontFeatures::default()
    );
}

#[test]
fn invalid_feature_values_fail_atomically_instead_of_enabling_a_prefix() {
    for value in [
        "",
        "liga",
        "'abc'",
        "'abcde'",
        "'abé'",
        "'a\nbc'",
        "'liga' -1",
        "'liga' 1.5",
        "'liga' 1px",
        "'liga' bogus",
        "'liga',",
        "'liga', invalid",
        "normal, 'liga'",
        "'liga' on off",
        "'liga' !important",
    ] {
        assert!(FontFeatures::parse(value).is_none(), "{value}");
    }
    assert!(FontFeatures::parse(&vec!["'liga'"; MAX_FONT_FEATURES + 1].join(",")).is_none());
    assert!(FontFeatures::from_settings(&[([0, 0, 0, 0], 1)]).is_none());
    assert!(FontFeatures::from_settings(&vec![(*b"kern", 0); MAX_FONT_FEATURES + 1]).is_none());
}

#[test]
fn kerning_tokenization_rejects_extra_tokens_and_decodes_css_escapes() {
    assert_eq!(FontKerning::parse(r"n\6f ne"), Some(FontKerning::None));
    assert_eq!(
        FontKerning::parse("/* comment */ normal"),
        Some(FontKerning::Normal)
    );
    for value in ["", "normal none", "false", "1", "inherit"] {
        assert!(FontKerning::parse(value).is_none());
    }
}

#[test]
fn specified_features_retain_order_duplicates_and_omit_the_default_one_value() {
    for (input, expected) in [
        ("'dlig' 1", r#""dlig""#),
        ("'smcp' on", r#""smcp""#),
        ("'liga' off", r#""liga" 0"#),
        ("'tnum', 'hist'", r#""tnum", "hist""#),
        ("'dlig' 1,'smcp' on,'dlig' 0", r#""dlig", "smcp", "dlig" 0"#),
        (r"'\22\7d\2f\2a'", r#""\"}/*""#),
        (r"'\22\22\22\22'", r#""\"\"\"\"""#),
        (r"'\\abc'", r#""\\abc""#),
    ] {
        assert_eq!(
            FontFeatures::specified_css_text(input).as_deref(),
            Some(expected),
            "{input}"
        );
        assert!(
            FontFeatures::parse(expected).is_some(),
            "round trip {expected}"
        );
    }
    let value = "'dlig' 1,'smcp' on,'dlig' 0";
    assert_eq!(
        FontFeatures::parse(value).unwrap().css_text(),
        r#""dlig" 0, "smcp""#
    );
}

#[test]
fn feature_serialization_preserves_every_printable_ascii_tag_byte() {
    for byte in 0x20..=0x7e {
        let tag = [byte, b'a', b'b', b'c'];
        let features = FontFeatures::from_settings(&[(tag, 1)]).unwrap();
        let serialized = features.css_text();
        assert_eq!(FontFeatures::parse(&serialized).unwrap(), features);
        assert_eq!(
            FontFeatures::specified_css_text(&serialized).unwrap(),
            serialized
        );
    }
    let max = FontFeatures::parse("'liga' 2147483648").unwrap();
    assert_eq!(
        max.settings(),
        &[(*b"liga", i32::MAX as u32)],
        "CSS integer token clamping"
    );
}
