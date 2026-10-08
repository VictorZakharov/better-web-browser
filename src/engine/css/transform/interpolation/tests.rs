use super::*;

fn resolved(from: &str, to: &str, progress: f64, width: f32, height: f32, font: f32) -> (f32, f32) {
    let value = interpolate(from, to, progress).unwrap();
    parse_transform(&value)
        .unwrap()
        .resolve(width, height, font)
}

#[test]
fn comparisons_are_resolved_after_interpolation_against_the_actual_box() {
    let from = "translate(min(50%, 30px), 0px)";
    let to = "translate(max(20%, 50px), 40px)";
    assert_eq!(resolved(from, to, 0.5, 100.0, 80.0, 16.0), (40.0, 20.0));
    assert_eq!(resolved(from, to, 0.5, 400.0, 80.0, 16.0), (55.0, 20.0));
}

#[test]
fn mismatched_translation_lists_keep_relative_lengths() {
    assert_eq!(
        resolved("translateX(1em)", "translateY(25%)", 0.5, 100.0, 80.0, 20.0),
        (10.0, 10.0)
    );
    assert_eq!(
        resolved(
            "translateX(10px) translateY(20px)",
            "translate(30px, 40px)",
            0.5,
            100.0,
            80.0,
            16.0
        ),
        (20.0, 30.0)
    );
    assert_eq!(
        resolved("none", "translate(2em, 50%)", 0.5, 100.0, 80.0, 20.0),
        (20.0, 20.0)
    );
}

#[test]
fn endpoints_and_finite_overshoot_preserve_animation_semantics() {
    assert_eq!(
        interpolate("none", "translateX(10px)", 0.0).unwrap(),
        "none"
    );
    assert_eq!(
        resolved("none", "translateX(10px)", 1.0, 100.0, 80.0, 16.0),
        (10.0, 0.0)
    );
    assert_eq!(
        resolved("none", "translateX(10px)", -0.5, 100.0, 80.0, 16.0),
        (-5.0, 0.0)
    );
    assert_eq!(interpolate("none", "none", 0.5).unwrap(), "none");
    for progress in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
        assert!(interpolate("none", "translateX(10px)", progress).is_none());
    }
}

#[test]
fn unsupported_transforms_do_not_acquire_a_fake_matrix_fallback() {
    for value in [
        "rotate(10deg)",
        "scale(2)",
        "translateX(calc(1s))",
        "translate(1px, auto)",
    ] {
        assert!(interpolate(value, "none", 0.5).is_none(), "{value}");
        assert!(interpolate("none", value, 0.5).is_none(), "{value}");
    }
}
