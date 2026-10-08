use super::*;

#[test]
fn mathematical_bezier_coordinates_are_resolved_and_type_checked() {
    for (source, expected) in [
        (
            "cubic-bezier(calc(1 / 4), min(0, 1), max(.5, .75), sqrt(1))",
            "cubic-bezier(0.25, 0, 0.75, 1)",
        ),
        ("cubic-bezier(calc(-1), 0, 1, 1)", ""),
        ("cubic-bezier(calc(1px), 0, 1, 1)", ""),
        ("cubic-bezier(25%, 0, 1, 1)", ""),
        (
            "cubic-bezier(0, calc(infinity), 1, calc(-infinity))",
            "cubic-bezier(0, 340282350000000000000000000000000000000, 1, -340282350000000000000000000000000000000)",
        ),
    ] {
        if expected.is_empty() {
            assert!(normalize(source).is_none(), "{source}")
        } else {
            assert_eq!(normalize(source).as_deref(), Some(expected), "{source}")
        }
    }
}

#[test]
fn calculated_steps_round_integer_ties_before_range_checks() {
    for (source, expected) in [
        ("steps(calc(2.5), jump-none)", "steps(3, jump-none)"),
        ("steps(round(4.1), start)", "steps(4, start)"),
        ("steps(sqrt(16))", "steps(4, end)"),
        ("steps(calc(16777217), end)", "steps(16777217, end)"),
        ("steps(calc(-1))", "steps(1, end)"),
        ("steps(calc(0))", "steps(1, end)"),
        ("steps(calc(NaN))", "steps(1, end)"),
    ] {
        assert_eq!(normalize(source).as_deref(), Some(expected), "{source}")
    }
    for source in [
        "steps(-1)",
        "steps(0)",
        "steps(calc(1.4), jump-none)",
        "steps(1.5)",
        "steps(1e0)",
        "steps(calc(1s))",
        "steps(4, sideways)",
        "steps(4,)",
    ] {
        assert!(normalize(source).is_none(), "{source}");
    }
}

#[test]
fn linear_math_preserves_contiguous_position_groups_and_nested_commas() {
    for (source, expected) in [
        (
            "linear(calc(0), sqrt(.25) min(20%, 30%) max(70%, 50%), calc(1))",
            "linear(0, 0.5 20% 70%, 1)",
        ),
        (
            "linear(calc(0%) calc(20%) min(0, 1), max(80%, 60%) calc(100%) sqrt(1))",
            "linear(0 0% 20%, 1 80% 100%)",
        ),
        (
            "linear(calc(-1) -10%, calc(2) 110%)",
            "linear(-1 -10%, 2 110%)",
        ),
    ] {
        assert_eq!(normalize(source).as_deref(), Some(expected), "{source}")
    }
    for source in [
        "linear(calc(0))",
        "linear(0, calc(1px))",
        "linear(calc(0%) 0 calc(20%), 1)",
        "linear(calc(0) calc(1), 1)",
        "linear(0 calc(10%),)",
        "linear(0 10% 20% 30%, 1)",
        "linear(0 calc(10% + 1px), 1)",
        "linear(0 calc(10% + 1), 1)",
        "linear(sign(1em), 1)",
    ] {
        assert!(normalize(source).is_none(), "{source}")
    }
}
