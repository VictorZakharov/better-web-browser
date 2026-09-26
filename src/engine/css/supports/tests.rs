use super::*;

#[test]
fn feature_queries_are_conservative_about_unimplemented_values() {
    assert!(supports_matches("@supports (display: grid)"));
    assert!(supports_matches("@supports (position: sticky)"));
    assert!(!supports_matches(
        "@supports (grid-template-columns: subgrid)"
    ));
    assert!(supports_matches("@supports (justify-self: center)"));
    assert!(supports_matches("@supports (opacity: 25%)"));
    assert!(supports_matches("@supports (z-index: -12)"));
    assert!(supports_matches("@supports (z-index: auto)"));
    assert!(!supports_matches("@supports (z-index: 1.5)"));
}

#[test]
fn condition_validity_is_distinct_from_feature_support() {
    for valid in [
        "(display: grid)",
        "(unimplemented-property: future-value)",
        "not (unimplemented-property: future-value)",
        "selector(:future-pseudo)",
        "not (future-feature)",
    ] {
        assert!(supports_condition_valid(valid), "valid: {valid}");
    }
    for invalid in [
        "",
        "display: grid",
        "not (display: grid) trailing",
        "(display: grid) and",
        "not (display: grid",
        "(display: grid) and (position: sticky) or (color: red)",
    ] {
        assert!(!supports_condition_valid(invalid), "invalid: {invalid}");
    }
    assert!(!supports_matches("(unimplemented-property: future-value)"));
    assert!(supports_condition_valid(
        "(unimplemented-property: future-value)"
    ));
}

#[test]
fn import_declaration_validity_is_not_support_status_or_general_enclosed() {
    for valid in [
        "display: grid",
        "unimplemented-property: future-value",
        "display: grid !important",
        "--custom:",
    ] {
        assert!(supports_import_declaration_valid(valid), "valid: {valid}");
    }
    for invalid in [
        "",
        "not (display: grid)",
        "(display: grid) and",
        "display: grid; color: red",
        "display: red ! no",
        "display: calc(1px",
    ] {
        assert!(
            !supports_import_declaration_valid(invalid),
            "invalid: {invalid}"
        );
    }
}

#[test]
fn feature_queries_accept_valid_variable_references_without_resolving_them() {
    assert!(supports_matches("@supports (color: var(--test, red))"));
    assert!(supports_matches("@supports (position: var(--position))"));
    assert!(supports_matches(
        "@supports (width: calc(var(--space, var(--fallback)) * 2))"
    ));
    assert!(supports_matches("@supports (color: var(--empty,))"));
    assert!(!supports_matches("@supports (box-shadow: var(--shadow))"));
    assert!(!supports_matches("@supports (color: var(color, red))"));
    assert!(!supports_matches("@supports (color: var())"));
}

#[test]
fn two_argument_supports_uses_literal_property_names_and_value_grammar() {
    for (property, value) in [
        ("display", "grid"),
        ("DISPLAY", "grid"),
        ("width", " 5px "),
        ("display", "grid/**/"),
        ("display", "/**/grid"),
        ("display", "g\\72id"),
        ("width", "5\\70x"),
        ("color", "r\\65 d"),
        ("--accent", ""),
        ("--accent", " '!important' "),
        ("--accent", "fn(!important)"),
    ] {
        assert!(
            supports_declaration_value(property, value),
            "{property}: {value}"
        );
    }
    for (property, value) in [
        (" display", "grid"),
        ("display ", "grid"),
        ("--", "value"),
        ("--a b", "value"),
        ("--a\\62", "value"),
        ("display", "grid !important"),
        ("display", "grid ! IMPORTANT"),
        ("display", "grid !/**/important"),
        ("display", "gr/**/id"),
        ("width", "5/**/px"),
        ("width", "5 px"),
        ("opacity", "25 %"),
        ("font-size", "5 px"),
        ("line-height", "5 px"),
        ("--accent", "!important"),
        ("--accent", "red ! no"),
        ("--accent", "red; blue"),
        ("--accent", "'unterminated"),
    ] {
        assert!(
            !supports_declaration_value(property, value),
            "{property}: {value}"
        );
    }
}

#[test]
fn boolean_grammar_requires_grouped_operands_and_one_operator_per_level() {
    for supported in [
        "(display: grid) and (position: sticky)",
        "(missing: value) or (display: grid)",
        "not (missing: value)",
        "((display: grid) and ((position: sticky) or (missing: value)))",
        "not ((missing: value) and (display: grid))",
        "(display: grid) AND (position: sticky)",
        "not foo(bar)",
        "not ()",
        // A semicolon makes this an unsupported general-enclosed expression, not an invalid
        // whole supports query. General-enclosed is false, and `not` may invert it.
        "not (color: red;)",
        "not (display: grid; color: red)",
    ] {
        assert!(supports_matches(supported), "{supported}");
    }
    for unsupported in [
        "(display: grid) and (missing: value)",
        "(missing: value) or (also-missing: value)",
        "not (display: grid)",
        "((display: grid) and (missing: value))",
        "future-feature(foo)",
    ] {
        assert!(!supports_matches(unsupported), "{unsupported}");
    }
    for invalid in [
        "not not (display: grid)",
        "not (display: grid) and (position: sticky)",
        "(display: grid) and (position: sticky) or (color: red)",
        "(display: grid) or (position: sticky) and (color: red)",
        "(display: grid)and (position: sticky)",
        "(display: grid) or(display: flex)",
        "not(display: grid)",
        "(display: grid) and display: flex",
        "not (display: grid",
        "not (color: red) garbage",
        "not /* unterminated comment",
    ] {
        assert!(
            !supports_matches(invalid),
            "invalid query accepted: {invalid}"
        );
    }
}

#[test]
fn css_tokens_in_queries_preserve_comments_strings_and_declaration_boundaries() {
    assert!(supports_matches("(display/**/:/**/grid)"));
    assert!(supports_matches("(color: rgb(0, 0, 0))"));
    assert!(supports_matches("(--custom:)"));
    assert!(supports_matches("(--custom: arbitrary tokens)"));
    assert!(supports_matches("(display: grid !important)"));
    assert!(supports_matches("(display: grid ! IMPORTANT)"));
    assert!(supports_matches("(display: grid /**/ ! /**/ important)"));
    assert!(supports_matches("(--custom: !important)"));
    assert!(supports_matches("(--custom: ! important)"));
    assert!(supports_matches("(--custom: '!important')"));
    assert!(supports_matches("(--custom: fn(!important))"));
    assert!(!supports_matches("not (color: red !important)"));
    for invalid in [
        "(dis/**/play: grid)",
        "(display: gr/**/id)",
        "(display: grid; color: red)",
        "(--custom: 1; color: red)",
        "(--custom: value ! unknown)",
        "(--custom: value !important trailing)",
        "(--: value)",
        "not (color: 'unterminated)",
    ] {
        assert!(!supports_matches(invalid), "{invalid}");
    }
}

#[test]
fn selector_queries_do_not_advertise_unknown_or_forgiven_selector_parts() {
    for supported in [
        "selector(.card)",
        "selector(main > .card:hover)",
        "selector(input:focus)",
        "selector(section:has(> .card))",
        "selector(::before)",
        "selector(.card:is(.ready, #chosen))",
        "selector(.card:nth-child(2n of .ready))",
    ] {
        assert!(supports_matches(supported), "{supported}");
    }
    for unsupported in [
        "selector(:future-pseudo)",
        "selector(.card:is(.ready, :future-pseudo))",
        "selector(.card:where(.ready, :future-pseudo))",
        "selector(.card:is(.ready, :where(:future-pseudo)))",
        "selector(:has(:has(.ready)))",
        "selector(:has(:is(.ready, :has(.child))))",
        "selector(:has(:not(:has(.child))))",
        "selector(.card:focus-visible)",
        "selector(::file-selector-button)",
        "selector(col || td)",
        "selector(.first, .second)",
        "selector(&)",
        "selector()",
    ] {
        assert!(!supports_matches(unsupported), "{unsupported}");
    }
}

#[test]
fn conditional_styles_apply_only_for_supported_and_well_formed_queries() {
    let dom = dom::parse(
        r#"<style>
            .supported, .invalid, .unsupported { display: none }
            @supports selector(.card:has(> .ready)) and (display: grid) {
                .supported { display: block }
            }
            @supports not (display: grid) or (position: sticky) {
                .invalid { display: block }
            }
            @supports selector(.card:is(.ready, :future-pseudo)) {
                .unsupported { display: block }
            }
        </style>
        <div class=supported></div><div class=invalid></div><div class=unsupported></div>"#,
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let display = |class| {
        let node = dom
            .elements_named("div")
            .find(|node| node.has_class(class))
            .unwrap();
        styles.get(&node).display
    };
    assert_eq!(display("supported"), Display::Block);
    assert_eq!(display("invalid"), Display::None);
    assert_eq!(display("unsupported"), Display::None);
}
