use super::*;

#[test]
fn fuzz_regression_non_ascii_attribute_suffix_does_not_panic() {
    // The garbage suffix is invalid CSS, but must not trigger byte slicing at
    // a non-ASCII boundary.
    assert!(parse_attribute_selector("data-value=\"x\"�").is_none());
}

#[test]
fn attribute_modifiers_accept_css_whitespace_without_byte_slicing() {
    assert!(
        parse_attribute_selector("data-value=\"x\"\tI")
            .unwrap()
            .case_sensitivity
            == AttributeCaseSensitivity::AsciiInsensitive
    );
    assert!(
        parse_attribute_selector("data-value=\"x\"\nS")
            .unwrap()
            .case_sensitivity
            == AttributeCaseSensitivity::Sensitive
    );
}

#[test]
fn generated_pseudo_elements_use_the_originating_selector() {
    let (selector, pseudo) = parse_style_rule_selector(".card:before").unwrap();
    assert_eq!(pseudo, Some(PseudoElement::Before));
    assert_eq!(selector.specificity.classes, 1);
    assert_eq!(selector.specificity.tags, 1);

    let (selector, pseudo) = parse_style_rule_selector("#footer::AFTER").unwrap();
    assert_eq!(pseudo, Some(PseudoElement::After));
    assert_eq!(selector.specificity.ids, 1);
    assert_eq!(selector.specificity.tags, 1);
    assert!(parse_style_rule_selector(".card::marker").is_none());
}

#[test]
fn forgiving_lists_ignore_unsupported_members_but_not_is_unforgiving() {
    assert!(parse_selector("p:is(.ready, :unsupported)").is_some());
    assert!(parse_selector("p:where(.ready, :unsupported)").is_some());
    assert!(parse_selector("p:not(.ready, :unsupported)").is_none());
    assert!(parse_selector("p:not(.ready,)").is_none());
}

#[test]
fn functional_selector_depth_is_bounded() {
    let mut source = String::from("p");
    for _ in 0..12 {
        source = format!(":not({source})");
    }
    assert!(parse_selector(&source).is_none());
}

#[test]
fn malformed_combinators_invalidate_the_selector() {
    assert!(parse_selector("> p").is_none());
    assert!(parse_selector("div >").is_none());
    assert!(parse_selector("div > > p").is_none());
    assert!(parse_selector("div + p").is_some());
}

#[test]
fn nth_child_parses_an_plus_b_and_complex_of_lists() {
    for source in [
        "li:nth-child(odd)",
        "li:nth-child(2n+1)",
        "li:nth-child(-n+2 of .marked, #special > li)",
        "li:nth-last-child(2 of .marked)",
        "li:nth-of-type(even)",
        "li:nth-last-of-type(3n-1)",
    ] {
        assert!(parse_selector(source).is_some(), "{source}");
    }
    let selector = parse_selector("li:nth-child(2 of .marked, #special > li)").unwrap();
    assert_eq!(selector.specificity.ids, 1);
    assert_eq!(selector.specificity.classes, 1);
    assert_eq!(selector.specificity.tags, 2);
}

#[test]
fn nth_child_rejects_invalid_syntax_and_of_type_filters() {
    for source in [
        "li:nth-child(foo)",
        "li:nth-child(2n+-1)",
        "li:nth-child(2 of)",
        "li:nth-child(2 of .marked,)",
        "li:nth-of-type(2 of .marked)",
    ] {
        assert!(parse_selector(source).is_none(), "{source}");
    }
}

#[test]
fn top_level_nesting_selector_is_zero_specificity_scope() {
    let selector = parse_selector("&").unwrap();
    assert!(selector.compounds[0].requires_scope);
    assert_eq!(selector.specificity, Specificity::default());

    let selector = parse_selector("&.active").unwrap();
    assert!(selector.compounds[0].requires_scope);
    assert_eq!(selector.specificity.classes, 1);
}

#[test]
fn escaped_identifiers_and_attribute_strings_parse_as_css_tokens() {
    let selector = parse_selector(r#"#\31 23.a\+b[data-label="a > b"]"#).unwrap();
    assert_eq!(selector.compounds[0].id.as_deref(), Some("123"));
    assert_eq!(selector.compounds[0].classes, ["a+b"]);
    assert_eq!(selector.compounds[0].attributes[0].value, "a > b");

    let attribute = parse_attribute_selector(r#"data-\6c abel="a ] b" i"#).unwrap();
    assert_eq!(attribute.name, "data-label");
    assert_eq!(attribute.value, "a ] b");
    assert_eq!(
        attribute.case_sensitivity,
        AttributeCaseSensitivity::AsciiInsensitive
    );
    for source in ["[data-x='unterminated]", "[data-x=1]", "[data-x=a bogus]"] {
        assert!(parse_selector(source).is_none(), "{source}");
    }
}

#[test]
fn language_ranges_require_css_identifiers_or_strings() {
    assert!(parse_selector(r#"p:lang(de-DE, "*-Latn", "")"#).is_some());
    assert!(parse_selector("p:lang(\\*-Latn)").is_some());
    for source in ["p:lang()", "p:lang(de,)", "p:lang(*-Latn)", "p:lang(de en)"] {
        assert!(parse_selector(source).is_none(), "{source}");
    }
}

#[test]
fn directionality_pseudo_accepts_only_ltr_or_rtl() {
    assert!(parse_selector("p:dir(ltr)").is_some());
    assert!(parse_selector("p:dir(RTL)").is_some());
    for source in ["p:dir()", "p:dir(auto)", "p:dir(ltr, rtl)"] {
        assert!(parse_selector(source).is_none(), "{source}");
    }
}
