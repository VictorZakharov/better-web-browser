use super::*;

fn scalar(source: &str) -> f32 {
    number(source).unwrap_or_else(|| panic!("invalid {source}"))
}

#[test]
fn signed_zero_survives_inside_tree_but_not_at_public_boundary() {
    for (source, negative) in [
        ("calc(1 / -0)", true),
        ("calc(1 / (-1 * 0))", true),
        ("calc(1 / min(0, -1 * 0))", true),
        ("calc(1 / max(0, -1 * 0))", false),
        ("calc(1 / clamp(0, -1 * 0, 1))", false),
        ("calc(1 / round(up, -.1, 1))", true),
        ("calc(1 / round(down, .1, 1))", false),
        ("calc(1 / sign(-1 * 0))", true),
        ("calc(1 / mod(10, -5))", true),
        ("calc(1 / rem(-10, 5))", true),
        ("calc(1 / abs(-1 * 0))", false),
    ] {
        let value = scalar(source);
        assert_eq!(value.abs(), f32::MAX, "{source}");
        assert_eq!(value.is_sign_negative(), negative, "{source}");
    }
    for source in ["calc(-1 * 0)", "round(up,-.1,1)", "sign(-1 * 0)"] {
        assert_eq!(scalar(source).to_bits(), 0.0f32.to_bits(), "{source}");
    }
}

#[test]
fn nan_is_infectious_until_outer_calculation_censors_it() {
    for source in [
        "calc(0 / 0)",
        "sqrt(-1)",
        "log(-1)",
        "log(2,1)",
        "log(2,-1)",
        "round(2,0)",
        "mod(2,0)",
        "rem(2,0)",
        "min(3,NaN)",
        "max(NaN,3)",
        "clamp(1,NaN,3)",
        "pow(NaN,0)",
        "hypot(infinity,NaN)",
        "calc(sqrt(-1) + 20)",
        "pow(-1,infinity)",
        "pow(1,-infinity)",
        "sin(infinity)",
        "sin(asin(2))",
    ] {
        assert_eq!(scalar(source), 0.0, "{source}");
    }
    assert_eq!(scalar("calc(sign(1 / 0) + 20)"), 21.0);
    assert_eq!(scalar("calc(sign(-infinity) + 20)"), 19.0);
    assert_eq!(scalar("calc(1 / infinity)"), 0.0);
}

#[test]
fn infinite_steps_follow_css_not_rust_remainder_shortcuts() {
    for (source, expected) in [
        ("round(-1,infinity)", 0.0),
        ("round(up,1,infinity)", f32::MAX),
        ("round(down,-1,infinity)", -f32::MAX),
        ("mod(10,infinity)", 10.0),
        ("mod(-10,-infinity)", -10.0),
        ("mod(-10,infinity)", 0.0),
        ("rem(-10,infinity)", -10.0),
        ("rem(10,-infinity)", 10.0),
        ("mod(infinity,1)", 0.0),
        ("round(infinity,infinity)", 0.0),
    ] {
        assert_eq!(scalar(source), expected, "{source}");
    }
}

#[test]
fn typed_angles_and_inverse_trigonometry_work_inside_lengths() {
    for (source, expected) in [
        ("calc(sin(90deg) * 100px)", 100.0),
        ("calc(cos(180deg) * 100px)", -100.0),
        ("calc(cos(.25turn) * 100px)", 0.0),
        ("calc(sin(100grad) * 100px)", 100.0),
        ("calc(sin(pi / 2) * 100px)", 100.0),
        ("calc(sin(asin(1)) * 100px)", 100.0),
        ("calc(cos(acos(-1)) * 100px)", -100.0),
        ("calc(tan(atan(2)) * 100px)", 200.0),
        ("calc(sin(atan2(3px,4px)) * 100px)", 60.0),
        ("calc(sin(atan2(30%,40%)) * 100px)", 60.0),
        ("calc((1turn / 90deg) * 100px)", 400.0),
        ("calc(pow(30px / 1px,2) * 1px)", 900.0),
    ] {
        let value = parse_length(source).unwrap().resolve(100.0, 16.0).unwrap();
        assert!((value - expected).abs() < 0.001, "{source}: {value}");
    }
    for source in [
        "calc(1px + 1deg)",
        "calc(sin(1px) * 10px)",
        "calc(asin(1deg) * 10px)",
        "calc(atan2(1px,1deg) * 10px)",
        "calc((1px / 1deg) * 10px)",
        "calc(1deg * 10px)",
    ] {
        assert!(parse_length(source).is_none(), "{source}");
    }
}

#[test]
fn number_consumers_do_not_guess_font_or_viewport_context() {
    assert!(number("sign(1em)").is_none());
    assert!(number("sign(50%)").is_none());
    assert_eq!(number("sign(10px)"), Some(1.0));
    assert_eq!(percentage("round(55%, 10%)"), Some(0.6));
    assert_eq!(percentage("calc(100% / 4)"), Some(0.25));
    for source in ["calc(25% + .25)", "min(20%,20px)", "calc(1em)"] {
        assert!(percentage(source).is_none(), "{source}");
    }
}
