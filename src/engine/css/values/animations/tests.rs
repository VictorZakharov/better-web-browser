use super::*;

#[test]
fn initial_settings_are_non_inherited_level_one_values() {
    let settings = AnimationSettings::default();
    for (property, expected) in PROPERTIES
        .iter()
        .zip(["none", "0s", "0s", "ease", "1", "normal", "none", "running"])
    {
        assert_eq!(settings.property_value(property).as_deref(), Some(expected));
    }
}

#[test]
fn shorthand_is_order_independent_and_resets_all_longhands() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation",
        "Slide 250ms linear -50ms 2.5 alternate both paused"
    ));
    assert_eq!(settings.names, [AnimationName::Named("Slide".into())]);
    assert_eq!(settings.durations, [0.25]);
    assert_eq!(settings.delays, [-0.05]);
    assert_eq!(settings.iterations, [2.5]);
    assert_eq!(settings.easings, ["linear"]);
    assert_eq!(settings.directions, ["alternate"]);
    assert_eq!(settings.fills, ["both"]);
    assert_eq!(settings.states, ["paused"]);
    assert!(apply(&mut settings, "animation", "other"));
    assert_eq!(settings.durations, [0.0]);
    assert_eq!(settings.delays, [0.0]);
    assert_eq!(settings.iterations, [1.0]);
    assert_eq!(settings.states, ["running"]);
}

#[test]
fn comma_lists_preserve_function_commas_and_case_sensitive_names() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation",
        "Spin 1s cubic-bezier(.1, .2, .3, .4), spin 2s steps(3, jump-end) infinite"
    ));
    assert_eq!(
        settings.names,
        [
            AnimationName::Named("Spin".into()),
            AnimationName::Named("spin".into())
        ]
    );
    assert_eq!(
        settings.easings,
        ["cubic-bezier(.1, .2, .3, .4)", "steps(3, jump-end)"]
    );
    assert_eq!(settings.iterations, [1.0, f64::INFINITY]);
}

#[test]
fn quoted_names_are_not_interpreted_as_keywords() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation-name",
        "none, \"none\", \"initial\", \"with spaces\""
    ));
    assert_eq!(
        settings.names,
        [
            AnimationName::None,
            AnimationName::Quoted("none".into()),
            AnimationName::Quoted("initial".into()),
            AnimationName::Quoted("with spaces".into())
        ]
    );
    assert!(apply(&mut settings, "animation", "1s \"with spaces\""));
    assert_eq!(
        settings.names,
        [AnimationName::Quoted("with spaces".into())]
    );
    assert!(apply(&mut settings, "animation", "1s \"ease\""));
    assert_eq!(settings.easings, ["ease"]);
    assert_eq!(settings.names, [AnimationName::Quoted("ease".into())]);
}

#[test]
fn escaped_names_round_trip_through_cssom() {
    let name = AnimationName::parse(r"s\70 in").unwrap();
    assert_eq!(name, AnimationName::Named("spin".into()));
    assert_eq!(AnimationName::parse(&name.css_text()), Some(name));
    for value in ["\"comma,name\"", "\"a\\\"b\"", "\"line\\a break\""] {
        let name = AnimationName::parse(value).unwrap();
        assert_eq!(
            AnimationName::parse(&name.css_text()).unwrap().named(),
            name.named()
        );
    }
}

#[test]
fn invalid_values_never_partially_replace_prior_settings() {
    let mut settings = AnimationSettings::default();
    assert!(apply(&mut settings, "animation", "Valid 2s linear both"));
    let previous = settings.clone();
    for (property, value) in [
        ("animation-duration", "1s, -2s"),
        ("animation-duration", "1 s"),
        ("animation-delay", "nan"),
        ("animation-iteration-count", "-1"),
        ("animation-iteration-count", "2px"),
        ("animation-name", "inherit, slide"),
        ("animation-name", "default"),
        ("animation-name", "a b"),
        ("animation", "a -1s"),
        ("animation", "a 1s 2s 3s"),
        ("animation", "a b"),
        ("animation", "a 1s, b -1s"),
        ("animation-timing-function", "steps(1, jump-none)"),
        ("animation-timing-function", "cubic-bezier(2, 0, 1, 1)"),
        ("animation-play-state", "stopped"),
        ("animation-fill-mode", "auto"),
        ("animation-direction", "sideways"),
    ] {
        assert!(
            !apply(&mut settings, property, value),
            "{property}: {value}"
        );
        assert_eq!(settings, previous, "{property}: {value}");
    }
}

#[test]
fn list_budget_is_bounded_and_atomic() {
    let mut settings = AnimationSettings::default();
    let at_limit = std::iter::repeat_n("a", 64).collect::<Vec<_>>().join(",");
    assert!(apply(&mut settings, "animation-name", &at_limit));
    let previous = settings.clone();
    assert!(!apply(&mut settings, "animation-name", &(at_limit + ",b")));
    assert_eq!(settings, previous);
}

#[test]
fn css_wide_copy_changes_only_the_requested_component() {
    let mut settings = AnimationSettings::default();
    assert!(apply(
        &mut settings,
        "animation",
        "fade 1s -2s reverse paused"
    ));
    let mut next = AnimationSettings::default();
    assert!(next.copy_property(&settings, "animation-delay"));
    assert_eq!(next.delays, [-2.0]);
    assert_eq!(next.names, [AnimationName::None]);
    assert_eq!(next.durations, [0.0]);
    assert!(next.copy_property(&settings, "animation"));
    assert_eq!(next, settings);
}

#[test]
fn static_computed_styles_share_initial_lists_and_edits_are_copy_on_write() {
    use crate::engine::css::{ComputedStyle, DeclarationContext, properties::apply_declaration};
    use std::sync::Arc;
    let original = ComputedStyle::initial();
    let mut changed = original.clone();
    assert!(Arc::ptr_eq(&original.animation, &changed.animation));
    let initial = ComputedStyle::initial();
    assert!(Arc::ptr_eq(&initial.animation, &original.animation));
    let context = || DeclarationContext {
        parent: None,
        lower_origin: &initial,
        layer_start: &initial,
        base_url: "https://example.test/",
        viewport_width: 800.0,
        viewport_height: 600.0,
    };
    apply_declaration(&mut changed, ("color", "red"), context());
    assert!(Arc::ptr_eq(&original.animation, &changed.animation));
    apply_declaration(&mut changed, ("animation-name", "fade"), context());
    assert!(!Arc::ptr_eq(&original.animation, &changed.animation));
    assert_eq!(original.animation.names, [AnimationName::None]);
    assert_eq!(
        changed.animation.names,
        [AnimationName::Named("fade".into())]
    );
}
