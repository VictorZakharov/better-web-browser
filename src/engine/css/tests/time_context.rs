//! Relative scalar subexpressions in <time> use computed font/viewport context.
use super::*;

fn computed(markup: &str, tag: &str) -> ComputedStyle {
    let dom = dom::parse(markup);
    let styles =
        StyleSet::from_sources_for_viewport(&dom, "https://example.test/", &[], 800.0, 600.0);
    styles.get(&dom.elements_named(tag).next().unwrap()).clone()
}

#[test]
fn time_lists_compute_each_component_with_final_font_and_viewport() {
    for order in [true, false] {
        let times = "animation-duration:calc(1em / 10px * 1s),500ms,calc(1vw / 1px * 1s);animation-delay:calc(-1em / 10px * 1s),calc(1vh / 1px * 1s);transition-duration:calc(1em / 10px * 1s),500ms;transition-delay:calc(-1em / 10px * 1s)";
        let declarations = if order {
            format!("font-size:30px;{times}")
        } else {
            format!("{times};font-size:30px")
        };
        let style = computed(&format!("<div style='{declarations}'></div>"), "div");
        assert_eq!(style.animation.durations, [3.0, 0.5, 8.0]);
        assert_eq!(style.animation.delays, [-3.0, 6.0]);
        assert_eq!(style.transition.durations, [3.0, 0.5]);
        assert_eq!(style.transition.delays, [-3.0]);
        assert!(!style.animation.calculated_times.has_pending());
        assert!(!style.transition.calculated_times.has_pending());
    }
}

#[test]
fn shorthands_admit_contextual_times_without_reordering_delay_and_duration() {
    let style = computed(
        "<div style='font-size:30px;animation:fade calc(1em / 10px * 1s) linear calc(-1em / 20px * 1s) both paused;transition:opacity calc(1em / 10px * 1s) ease calc(-1em / 20px * 1s)'></div>",
        "div",
    );
    assert_eq!(style.animation.durations, [3.0]);
    assert_eq!(style.animation.delays, [-1.5]);
    assert_eq!(style.animation.names[0].named(), Some("fade"));
    assert_eq!(style.animation.fills, ["both"]);
    assert_eq!(style.animation.states, ["paused"]);
    assert_eq!(style.transition.durations, [3.0]);
    assert_eq!(style.transition.delays, [-1.5]);
    assert_eq!(style.transition.properties, ["opacity"]);
}

#[test]
fn negative_calculated_durations_clamp_but_delays_remain_signed() {
    let style = computed(
        "<div style='font-size:30px;animation-duration:calc(-1em / 10px * 1s);animation-delay:calc(-1em / 10px * 1s);transition-duration:calc(-1em / 10px * 1s);transition-delay:calc(-1em / 10px * 1s)'></div>",
        "div",
    );
    assert_eq!(style.animation.durations, [0.0]);
    assert_eq!(style.animation.delays, [-3.0]);
    assert_eq!(style.transition.durations, [0.0]);
    assert_eq!(style.transition.delays, [-3.0]);
}

#[test]
fn invalid_times_never_partially_replace_pending_lists() {
    let style = computed(
        "<div style='font-size:30px;animation-duration:calc(1em / 10px * 1s),500ms;animation-duration:1s,calc(1em * 1s);transition-delay:calc(-1em / 10px * 1s),500ms;transition-delay:1s,calc(1em * 1s)'></div>",
        "div",
    );
    assert_eq!(style.animation.durations, [3.0, 0.5]);
    assert_eq!(style.transition.delays, [-3.0, 0.5]);
    for property in [
        "animation-duration",
        "animation-delay",
        "transition-duration",
        "transition-delay",
    ] {
        assert!(supports::supports_declaration_value(
            property,
            "calc(sign(1em) * 1s)"
        ));
        for value in [
            "calc(1em * 1s)",
            "calc(1em / 1px)",
            "calc(sign(1%) * 1s)",
            "1s,broken",
            "calc(1s + 1px)",
        ] {
            assert!(
                !supports::supports_declaration_value(property, value),
                "{property}:{value}"
            );
        }
    }
}

#[test]
fn inheritance_copies_computed_time_instead_of_parent_authored_units() {
    let style = computed(
        "<div style='font-size:30px;animation-duration:calc(1em / 10px * 1s);animation-delay:calc(-1em / 20px * 1s);transition-duration:calc(1em / 10px * 1s);transition-delay:calc(-1em / 20px * 1s)'><span style='font-size:10px;animation:inherit;transition:inherit'></span></div>",
        "span",
    );
    assert_eq!(style.animation.durations, [3.0]);
    assert_eq!(style.animation.delays, [-1.5]);
    assert_eq!(style.transition.durations, [3.0]);
    assert_eq!(style.transition.delays, [-1.5]);
}

#[test]
fn literal_and_initial_overrides_clear_pending_time_contexts() {
    for reset in [
        "animation-duration:500ms;animation-delay:250ms;transition-duration:500ms;transition-delay:250ms",
        "animation:initial;transition:initial",
    ] {
        let style = computed(
            &format!(
                "<div style='font-size:30px;animation:fade calc(1em / 10px * 1s) calc(-1em / 20px * 1s);transition:opacity calc(1em / 10px * 1s) calc(-1em / 20px * 1s);{reset}'></div>"
            ),
            "div",
        );
        let initial = reset.contains("initial");
        assert_eq!(style.animation.durations, [if initial { 0.0 } else { 0.5 }]);
        assert_eq!(style.animation.delays, [if initial { 0.0 } else { 0.25 }]);
        assert_eq!(
            style.transition.durations,
            [if initial { 0.0 } else { 0.5 }]
        );
        assert_eq!(style.transition.delays, [if initial { 0.0 } else { 0.25 }]);
    }
}
