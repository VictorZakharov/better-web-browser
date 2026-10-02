use super::*;

#[test]
fn authored_values_retain_units_and_variables_instead_of_resolving_them() {
    assert_eq!(value("width", "2em").as_deref(), Some("2em"));
    assert_eq!(
        value("opacity", "var(--amount, .4)").as_deref(),
        Some("var(--amount, .4)")
    );
    assert_eq!(
        value("animation-name", "var(--Name)").as_deref(),
        Some("var(--Name)")
    );
}

#[test]
fn animation_names_serialize_as_identifiers_except_reserved_keywords() {
    assert_eq!(
        value("animation-name", "\"two words\", \"none\", \"INITIAL\"").as_deref(),
        Some("two\\ words, \"none\", \"INITIAL\"")
    );
    assert_eq!(value("animation-name", "NONE").as_deref(), Some("none"));
    assert_eq!(value("animation-name", "\"\""), None);
}

#[test]
fn important_duplicates_keep_their_winner_and_property_order() {
    assert_eq!(
        list(
            "opacity:.2!important;color:red;opacity:.8;color:blue!important",
            false
        ),
        [
            ("opacity".into(), ".2".into(), true),
            ("color".into(), "blue".into(), true)
        ]
    );
}

#[test]
fn keyframes_ignore_priorities_and_animation_control_declarations() {
    assert_eq!(
        list(
            "opacity:.2;opacity:.8!important;animation-duration:2s;animation-timing-function:linear",
            true
        ),
        [
            ("opacity".into(), ".2".into(), false),
            ("animation-timing-function".into(), "linear".into(), false)
        ]
    );
}

#[test]
fn declarations_respect_quoted_semicolons_comments_and_function_arguments() {
    let declarations = list(
        "--text:'a;b:c';/* ;bad:one */ width:calc(10px + 2em); animation-timing-function:cubic-bezier(0,0,1,1)",
        false,
    );
    assert_eq!(declarations.len(), 3);
    assert_eq!(declarations[0], ("--text".into(), "'a;b:c'".into(), false));
    assert_eq!(declarations[1].1, "calc(10px + 2em)");
}

#[test]
fn invalid_values_cannot_smuggle_a_second_declaration() {
    for value in ["1; color:red", "1!important", "calc(", "var("] {
        assert!(super::value("opacity", value).is_none(), "{value}");
    }
    assert!(super::value("unknown-property", "anything").is_none());
}

#[test]
fn invalid_duplicate_leaves_a_valid_sibling_untouched() {
    assert_eq!(
        list(
            "opacity:.3;opacity:invalid;color:red;unsupported:stuff",
            false
        ),
        [
            ("opacity".into(), ".3".into(), false),
            ("color".into(), "red".into(), false)
        ]
    );
}

#[test]
fn cssom_value_budget_fails_closed_without_truncating_a_valid_token() {
    assert!(value("--large", &"x".repeat(MAX_CSSOM_VALUE_BYTES)).is_some());
    assert!(value("--large", &"x".repeat(MAX_CSSOM_VALUE_BYTES + 1)).is_none());
    assert!(value(&format!("--{}", "n".repeat(129)), "x").is_none());
}

#[test]
fn overflow_cssom_uses_native_axis_grammar_not_partial_capability_reporting() {
    for text in ["auto", "scroll", "clip auto", "visible hidden"] {
        assert_eq!(value("overflow", text).as_deref(), Some(text));
    }
    for text in ["bad", "auto hidden scroll", "hidden;opacity:1"] {
        assert!(value("overflow", text).is_none());
    }
    assert!(value("overflow-x", "auto hidden").is_none());
    assert!(!supports::supports_declaration_value("overflow", "auto"));
}
