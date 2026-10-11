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
        "rotateX(10deg)",
        "scaleZ(2)",
        "translateX(calc(1s))",
        "translate(1px, auto)",
    ] {
        assert!(interpolate(value, "none", 0.5).is_none(), "{value}");
        assert!(interpolate("none", value, 0.5).is_none(), "{value}");
    }
}

#[test]
fn primitive_animation_preserves_multiple_turns_and_scale_identity_padding() {
    let value = interpolate("none", "scaleX(.25)", 0.5).unwrap();
    assert_eq!(
        parse_transform(&value).unwrap().matrix(100.0, 80.0, 16.0).0,
        [0.625, 0.0, 0.0, 1.0, 0.0, 0.0]
    );
    let value = interpolate("rotate(0deg)", "rotate(720deg)", 0.25).unwrap();
    let (x, y) = parse_transform(&value)
        .unwrap()
        .matrix(0.0, 0.0, 16.0)
        .point(10.0, 0.0);
    assert!((x + 10.0).abs() < 0.00001 && y.abs() < 0.00001);
}
