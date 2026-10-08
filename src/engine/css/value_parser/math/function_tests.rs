use super::*;

#[test]
fn extended_functions_preserve_used_value_dependencies() {
    for (source, small, large) in [
        ("round(50%, 20px)", 60.0, 100.0),
        ("round(down, 50%, 20px)", 40.0, 100.0),
        ("mod(50%, 30px)", 20.0, 10.0),
        ("rem(50%, 30px)", 20.0, 10.0),
        ("abs(50% - 80px)", 30.0, 20.0),
        ("calc(sign(50% - 80px) * 20px)", -20.0, 20.0),
        ("hypot(30px, 40%)", 50.0, 85.44004),
    ] {
        let value = parse_length(source).unwrap_or_else(|| panic!("{source}"));
        assert!(value.has_percentage());
        let Length::Math(expression) = &value else {
            panic!("premature folding")
        };
        assert_eq!(expression.resolve(None, 16.0), None);
        for (basis, expected) in [(100.0, small), (200.0, large)] {
            assert!(
                (value.resolve(basis, 16.0).unwrap() - expected).abs() < 0.001,
                "{source}"
            );
        }
    }
}

#[test]
fn scalar_functions_can_scale_lengths_without_becoming_lengths() {
    for (source, expected) in [
        ("calc(pow(2, 3) * 1px)", 8.0),
        ("calc(sqrt(25) * 2px)", 10.0),
        ("calc(log(8, 2) * 10px)", 30.0),
        ("calc(log(e) * 10px)", 10.0),
        ("calc(exp(0) * 10px)", 10.0),
        ("calc(hypot(3, 4) * 10px)", 50.0),
        ("calc(round(2.5) * 10px)", 30.0),
        ("calc(round(to-zero, -2.5) * 10px)", -20.0),
        ("calc(min(2, 3) * 10px)", 20.0),
        ("calc(10px / max(1, 2))", 5.0),
    ] {
        assert!(
            (parse_length(source).unwrap().resolve(100.0, 16.0).unwrap() - expected).abs() < 0.001,
            "{source}"
        );
    }
    assert!(parse_length("sign(10px)").is_none());
    assert!(parse_length("hypot(3,4)").is_none());
    assert!(parse_length("calc(pi)").is_none());
    assert!(number("calc(pi)").is_some());
}

#[test]
fn function_arity_type_and_strategy_are_checked_before_cascade() {
    for source in [
        "round(20px)",
        "round(sideways,20px,2px)",
        "round(up 20px,2px)",
        "round(20px,2)",
        "mod(20px,2)",
        "rem(20px)",
        "abs(20px,10px)",
        "sign()",
        "hypot()",
        "hypot(20px,2)",
        "pow(2px,3)",
        "sqrt(25px)",
        "log(20px)",
        "exp(2px)",
        "calc(sign(20px) + 2px)",
        "calc(sign(20px) * sign(30px) * 2px * 3px)",
    ] {
        assert!(parse_length(source).is_none(), "accepted {source}");
    }
}

#[test]
fn extended_functions_roundtrip_and_map_all_relative_operands() {
    for source in [
        "round(up, 50%, 2em)",
        "mod(50%,2rem)",
        "abs(1vw - 50%)",
        "hypot(20%,2em,2rem)",
        "calc(sign(10% - 1vw) * 2rem)",
        "calc(pow(sign(10% - 20px), 2) * 5px)",
    ] {
        let value = parse_length(source)
            .unwrap()
            .resolve_root_font_units(20.0)
            .resolve_viewport_units(800.0, 600.0);
        let text = crate::engine::css::cssom::serialize_length(value.clone());
        let copy = parse_length(&text).unwrap_or_else(|| panic!("{text}"));
        for basis in [-100.0, 0.0, 100.0, 800.0] {
            assert_eq!(
                value.resolve(basis, 16.0),
                copy.resolve(basis, 16.0),
                "{text}"
            );
        }
    }
}
