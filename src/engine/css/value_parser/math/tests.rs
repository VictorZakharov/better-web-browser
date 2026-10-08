use super::*;

#[test]
fn normalization_reuses_unchanged_expression_storage_without_losing_context() {
    let original = parse_length("min(50%, max(20px, 2em))").unwrap();
    let Length::Math(tree) = &original else {
        panic!("comparison must remain deferred");
    };
    let normalized = original
        .clone()
        .resolve_root_font_units(10.0)
        .resolve_viewport_units(800.0, 600.0);
    let Length::Math(mapped) = &normalized else {
        panic!("percentage must remain deferred");
    };
    assert!(std::sync::Arc::ptr_eq(tree, mapped));
    assert_eq!(normalized.resolve(100.0, 16.0), Some(32.0));
    assert_eq!(normalized.resolve(100.0, 30.0), Some(50.0));
    let changed = parse_length("min(50%, 2rem)").unwrap();
    let root10 = changed.clone().resolve_root_font_units(10.0);
    let root30 = changed.resolve_root_font_units(30.0);
    assert_eq!(root10.resolve(100.0, 16.0), Some(20.0));
    assert_eq!(root30.resolve(100.0, 16.0), Some(50.0));
}

#[test]
fn desktop_viewport_variants_agree_in_literal_and_function_contexts() {
    // There is no retractable browser chrome in the desktop content viewport;
    // its small, large and dynamic dimensions coincide (CSS Values 4 §6.1.2).
    for (units, expected) in [
        (["vw", "svw", "lvw", "dvw"], 10.0),
        (["vh", "svh", "lvh", "dvh"], 6.0),
        (["vmin", "svmin", "lvmin", "dvmin"], 6.0),
        (["vmax", "svmax", "lvmax", "dvmax"], 10.0),
    ] {
        for unit in units {
            assert_eq!(pixels(&format!("1{unit}"), 1.0, 1000.0, 600.0), expected);
            assert_eq!(
                pixels(&format!("min(1{unit}, 100px)"), 1.0, 1000.0, 600.0),
                expected
            );
        }
    }
}

#[test]
fn canceling_percentages_preserves_type_and_indefinite_basis() {
    for source in ["calc(0%)", "calc(10% - 10%)", "calc(10% - 10% + 5px)"] {
        let value = parse_length(source).unwrap();
        assert!(value.has_percentage(), "lost type: {source}");
        let Length::Math(expression) = value else {
            panic!("percentage cancellation was prematurely simplified");
        };
        assert!(expression.resolve(None, 16.0).is_none());
        assert!(expression.resolve(Some(100.0), 16.0).is_some());
    }
}

fn pixels(source: &str, basis: f32, width: f32, height: f32) -> f32 {
    parse_length(source)
        .unwrap_or_else(|| panic!("invalid: {source}"))
        .resolve_viewport_units(width, height)
        .resolve(basis, 16.0)
        .unwrap()
}

#[test]
fn comparisons_choose_used_values_not_literal_magnitudes() {
    for (source, basis, expected) in [
        ("min(620px, 88vw)", 1000.0, 620.0),
        ("min(620px, 88vw)", 300.0, 620.0),
        ("min(50%, 200px)", 300.0, 150.0),
        ("min(50%, 200px)", 600.0, 200.0),
        ("max(10px, -20px, 2em)", 300.0, 32.0),
        ("clamp(100px, 50%, 300px)", 50.0, 100.0),
        ("clamp(100px, 50%, 300px)", 400.0, 200.0),
        ("clamp(100px, 50%, 300px)", 800.0, 300.0),
        ("clamp(300px, 20px, 100px)", 800.0, 300.0),
        ("min(620px, 88vw, (100vh - 190px) * 2.12)", 1000.0, 620.0),
        ("max(24px, 6vh)", 1000.0, 36.0),
        ("MIN(200px, MAX(20px, 10%))", 300.0, 30.0),
    ] {
        let actual = pixels(source, basis, 1000.0, 600.0);
        assert!((actual - expected).abs() < 0.001, "{source}: {actual}");
    }
    assert_eq!(pixels("min(620px, 88vw)", 300.0, 300.0, 200.0), 264.0);
}

#[test]
fn comparison_arguments_accept_arithmetic_and_nested_functions() {
    for (source, expected) in [
        ("min(100px + 20px, 200px)", 120.0),
        ("calc(min(100px, 50%) + 20px)", 120.0),
        ("calc(2 * max(10px, 1em))", 32.0),
        ("calc(min(100px, 50%) / 4)", 25.0),
        ("min(2 * (10px + 5px), 50px)", 30.0),
        ("min(1in, 100px)", 96.0),
        ("min(10px * max(2, 3), 100px)", 30.0),
        ("clamp(none, 40px, 20px)", 20.0),
        ("clamp(60px, 40px, none)", 60.0),
        ("clamp(none, 40px, none)", 40.0),
    ] {
        assert_eq!(pixels(source, 300.0, 1000.0, 600.0), expected, "{source}");
    }
}

#[test]
fn comparisons_preserve_indefinite_percentage_dependencies() {
    for source in ["min(10px, 50%)", "max(50%, 10px)", "clamp(10px, 50%, 30px)"] {
        let Length::Math(value) = parse_length(source).unwrap() else {
            panic!()
        };
        assert!(value.has_percentage());
        assert_eq!(value.resolve(None, 16.0), None, "{source}");
        assert!(value.resolve(Some(100.0), 16.0).is_some());
    }
    let Length::Math(value) = parse_length("min(10px, 20px)").unwrap() else {
        panic!()
    };
    assert_eq!(value.resolve(None, 16.0), Some(10.0));
}

#[test]
fn inconsistent_types_and_bad_arity_are_invalid() {
    for source in [
        "min()",
        "max()",
        "min(10px,)",
        "max(,10px)",
        "min(auto, 10px)",
        "min(10px, 2)",
        "min(0, 10px)",
        "min(10px + 2, 20px)",
        "min(10px * 20px, 100px)",
        "clamp(10px, 20px)",
        "clamp(10px, 20px, 30px, 40px)",
        "clamp(none, none, none)",
        "clamp(0, 20px, 30px)",
        "min(10px, 1fr)",
        "min(10px, 20px) junk",
        "min(10px, 20px)!important",
        "calc(min(10px, 20px) + bogus(1px))",
    ] {
        assert!(parse_length(source).is_none(), "accepted {source}");
    }
}

#[test]
fn expression_complexity_is_bounded_before_layout() {
    let deeply_nested = format!("{}1px{}", "min(".repeat(40), ")".repeat(40));
    assert!(parse_length(&deeply_nested).is_none());
    let wide = format!("min({})", vec!["1px"; 300].join(", "));
    assert!(parse_length(&wide).is_none());
    let numbers = format!("min({})", vec!["1"; 300].join(", "));
    assert!(parse_length(&numbers).is_none());
    let valid = format!("min({})", vec!["1px"; 32].join(", "));
    assert_eq!(pixels(&valid, 100.0, 100.0, 100.0), 1.0);
}

#[test]
fn relative_font_units_are_resolved_inside_all_branches() {
    let value = parse_length("min(2rem, max(10px, 1em))")
        .unwrap()
        .resolve_root_font_units(20.0);
    assert_eq!(value.resolve(100.0, 30.0), Some(30.0));
    assert_eq!(value.resolve(100.0, 50.0), Some(40.0));
}

#[test]
fn serialization_roundtrips_deferred_expressions() {
    for source in [
        "min(50%, 200px)",
        "max(10px, 2em)",
        "clamp(100px, 50%, 300px)",
        "calc(min(20px, 50%) * 2 + max(10px, 30px))",
        "clamp(none, 40px, 20px)",
    ] {
        let value = parse_length(source).unwrap();
        let text = crate::engine::css::cssom::serialize_length(value.clone());
        let reparsed = parse_length(&text).unwrap();
        for basis in [20.0, 100.0, 800.0] {
            assert_eq!(
                value.resolve(basis, 16.0),
                reparsed.resolve(basis, 16.0),
                "{text}"
            );
        }
    }
}

#[test]
fn css_tokens_not_substring_matching_select_math_functions() {
    for source in [
        "m\\69 n(20px, 30px)",
        "/* leading */ min(20px,30px)",
        "min(/* ) is a comment */20px,30px)",
        "calc(min(20px,30px) + 10px)",
    ] {
        let expected = if source.starts_with("calc") {
            30.0
        } else {
            20.0
        };
        assert_eq!(pixels(source, 100.0, 100.0, 100.0), expected, "{source}");
    }
}

#[test]
fn constant_comparisons_simplify_only_after_viewport_resolution() {
    let value = parse_length("min(620px,88vw)").unwrap();
    assert!(matches!(value, Length::Math(_)));
    assert_eq!(
        value.clone().resolve_viewport_units(1262.0, 539.0),
        Length::Px(620.0)
    );
    assert_eq!(
        value.resolve_viewport_units(500.0, 539.0),
        Length::Px(440.0)
    );
    let percentage = parse_length("min(620px,88%)")
        .unwrap()
        .resolve_viewport_units(1262.0, 539.0);
    assert!(matches!(percentage, Length::Math(_)));
}

#[test]
fn arithmetic_plus_and_minus_require_css_whitespace() {
    for source in [
        "min(10px+ 20px,40px)",
        "min(10px +20px,40px)",
        "min(10px/**/+/**/20px,40px)",
        "calc(10px+ 20px)",
        "calc(10px +20px)",
    ] {
        assert!(parse_length(source).is_none(), "accepted {source}");
    }
    for source in [
        "min(10px + 20px,40px)",
        "min(10px /* comment */ + /* comment */ 20px,40px)",
    ] {
        assert_eq!(pixels(source, 100.0, 100.0, 100.0), 30.0);
    }
}
