use super::*;

#[test]
fn longhand_times_compute_each_list_item_without_losing_comma_boundaries() {
    let mut settings = TransitionSettings::default();
    assert!(apply(
        &mut settings,
        "transition-duration",
        "min(1s,750ms), round(up,250ms,.1s), calc(-1s)"
    ));
    assert_eq!(settings.durations, [0.75, 0.3, 0.0]);
    assert!(apply(
        &mut settings,
        "transition-delay",
        "calc(-1s), rem(-250ms,.1s), hypot(3s,4000ms)"
    ));
    assert_eq!(settings.delays, [-1.0, -0.05, 5.0]);
    let serialized = serialize_times(&settings.delays);
    let mut next = TransitionSettings::default();
    assert!(apply(&mut next, "transition-delay", &serialized));
    assert_eq!(next.delays, settings.delays);
}

#[test]
fn shorthand_times_keep_duration_then_delay_order_and_atomic_replacement() {
    let mut settings = TransitionSettings::default();
    assert!(apply(
        &mut settings,
        "transition",
        "opacity round(up,250ms,.1s) linear calc(-1s), transform calc(-1s) ease .5s"
    ));
    assert_eq!(settings.properties, ["opacity", "transform"]);
    assert_eq!(settings.durations, [0.3, 0.0]);
    assert_eq!(settings.delays, [-1.0, 0.5]);
    assert!(apply(
        &mut settings,
        "transition",
        "/* duration */ calc(1s + 250ms) /**/ opacity /**/ ease-in calc(-50ms)"
    ));
    assert_eq!(settings.properties, ["opacity"]);
    assert_eq!(settings.durations, [1.25]);
    assert_eq!(settings.delays, [-0.05]);
    let saved = settings.clone();
    for (name, value) in [
        ("transition", "opacity -1s"),
        ("transition", "opacity calc(1s) 2s 3s"),
        ("transition", "opacity calc(1s + 1px)"),
        ("transition", "opacity calc(1s), transform -1s"),
        ("transition", "opacity calc(1s),"),
        ("transition-duration", "calc(1s), -1s"),
        ("transition-duration", "calc(1s), calc(1px)"),
        ("transition-delay", "calc(1s), calc(1%)"),
    ] {
        assert!(!apply(&mut settings, name, value), "{name}:{value}");
        assert_eq!(settings, saved, "{name}:{value}");
    }
}

#[test]
fn shared_scalar_constants_do_not_turn_into_timing_keywords() {
    let mut settings = TransitionSettings::default();
    assert!(apply(&mut settings, "transition-delay", "calc(NaN * 1s)"));
    assert_eq!(settings.delays, [0.0]);
    for source in ["NaNs", "infs", "infinitys", "0", "calc(0)", "sin(90deg)"] {
        assert!(!supports("transition-delay", source), "{source}");
    }
    assert!(supports("transition-delay", "calc(0s)"));
    assert!(supports("transition-duration", "calc(-1s)"));
    assert!(!supports("transition-duration", "-1s"));
}
