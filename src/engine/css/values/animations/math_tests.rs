use super::*;

#[test]
fn calculated_times_and_iteration_counts_compute_before_timeline_sampling() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation-duration",
        "min(1s,750ms), calc(-1s)"
    ));
    assert_eq!(settings.durations, [0.75, 0.0]);
    assert!(apply(
        &mut settings,
        "animation-delay",
        "calc(-1s), round(up,250ms,.1s)"
    ));
    assert_eq!(settings.delays, [-1.0, 0.3]);
    assert!(apply(
        &mut settings,
        "animation-iteration-count",
        "sqrt(4), calc(-1), infinite"
    ));
    assert_eq!(settings.iterations, [2.0, 0.0, f64::INFINITY]);
    assert!(apply(
        &mut settings,
        "animation-iteration-count",
        "calc(infinity)"
    ));
    // A calculated infinity clamps to the numeric range, not the keyword.
    assert_eq!(settings.iterations, [f64::from(f32::MAX)]);
    assert_ne!(
        settings
            .property_value("animation-iteration-count")
            .unwrap(),
        "infinite"
    );
    for name in [
        "animation-duration",
        "animation-delay",
        "animation-iteration-count",
    ] {
        let text = settings.property_value(name).unwrap();
        let mut next = AnimationSettings::default();
        assert!(apply(&mut next, name, &text), "{name}:{text}");
        assert_eq!(next.property_value(name), settings.property_value(name));
    }
}

#[test]
fn shorthand_keeps_case_sensitive_names_and_calculated_components_independent() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation",
        "Slide round(up,250ms,.1s) linear calc(-50ms) sqrt(4) alternate both paused, slide calc(-1s)"
    ));
    assert_eq!(
        settings.names,
        [
            AnimationName::Named("Slide".into()),
            AnimationName::Named("slide".into())
        ]
    );
    assert_eq!(settings.durations, [0.3, 0.0]);
    assert_eq!(settings.delays, [-0.05, 0.0]);
    assert_eq!(settings.iterations, [2.0, 1.0]);
    assert_eq!(settings.directions, ["alternate", "normal"]);
    assert_eq!(settings.states, ["paused", "running"]);
    let saved = settings.clone();
    for (name, value) in [
        ("animation", "Slide -1s"),
        ("animation", "Slide calc(1s) calc(2s) calc(3s)"),
        ("animation", "Slide calc(1s + 1px)"),
        ("animation", "Slide calc(1s), slide -1s"),
        ("animation-duration", "calc(1s), -1s"),
        ("animation-iteration-count", "sqrt(4), -1"),
        ("animation-iteration-count", "calc(1px)"),
        ("animation-iteration-count", "NaN"),
    ] {
        assert!(!apply(&mut settings, name, value), "{name}:{value}");
        assert_eq!(settings, saved, "{name}:{value}");
    }
}
