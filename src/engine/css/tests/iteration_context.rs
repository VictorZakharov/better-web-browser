//! Computed iteration lists retain the native animation clock's numeric contract.
use super::*;

fn computed(markup: &str, tag: &str) -> ComputedStyle {
    let dom = dom::parse(markup);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.get(&dom.elements_named(tag).next().unwrap()).clone()
}

#[test]
fn iteration_counts_use_final_font_in_both_orders_and_forms() {
    for font in [10, 20, 30, 40] {
        for form in [
            "animation:fade 1s calc(1em / 10px + .5)",
            "animation-iteration-count:calc(1em / 10px + .5)",
        ] {
            for declarations in [
                format!("{form};font-size:{font}px"),
                format!("font-size:{font}px;{form}"),
            ] {
                let style = computed(&format!("<div style='{declarations}'></div>"), "div");
                assert_eq!(style.animation.iterations, [font as f64 / 10.0 + 0.5]);
                assert!(style.animation.calculated_iterations.is_empty());
            }
        }
    }
}

#[test]
fn iteration_list_context_preserves_keyword_and_numeric_ranges() {
    let style = computed(
        "<div style='animation-iteration-count:calc(1em / 10px), infinite, calc(-1em / 10px), calc(infinity);font-size:30px'></div>",
        "div",
    );
    assert_eq!(
        style.animation.iterations,
        [3.0, f64::INFINITY, 0.0, f64::from(f32::MAX)]
    );
    assert_eq!(
        style.animation.property_value("animation-iteration-count"),
        Some(format!("3, infinite, 0, {}", f64::from(f32::MAX)))
    );
}

#[test]
fn inherited_iterations_do_not_reinterpret_parent_length_context() {
    let style = computed(
        "<div style='font-size:30px;animation-iteration-count:calc(1em / 10px), infinite'><span style='font-size:10px;animation-iteration-count:inherit'></span></div>",
        "span",
    );
    assert_eq!(style.animation.iterations, [3.0, f64::INFINITY]);
}

#[test]
fn iteration_literal_and_initial_overrides_clear_pending_trees() {
    for (suffix, expected) in [
        ("animation-iteration-count:4", 4.0),
        ("animation-iteration-count:initial", 1.0),
        ("animation:none", 1.0),
        ("all:initial", 1.0),
    ] {
        let style = computed(
            &format!(
                "<div style='animation-iteration-count:calc(1em / 10px);{suffix};font-size:30px'></div>"
            ),
            "div",
        );
        assert_eq!(style.animation.iterations, [expected]);
        assert!(style.animation.calculated_iterations.is_empty());
    }
}

#[test]
fn iteration_incompatible_types_and_invalid_lists_preserve_previous_winner() {
    for value in [
        "calc(1px)",
        "calc(1s)",
        "calc(10%)",
        "2, -1",
        "calc(1em / 10px), NaN",
        "calc(1em / 10px),",
    ] {
        assert!(!supports::supports_declaration_value(
            "animation-iteration-count",
            value
        ));
        let style = computed(
            &format!(
                "<div style='animation-iteration-count:4;animation-iteration-count:{value};font-size:30px'></div>"
            ),
            "div",
        );
        assert_eq!(style.animation.iterations, [4.0], "{value}");
    }
}

#[test]
fn authored_iteration_cssom_retains_unresolved_expression() {
    let mut settings = values::animations::AnimationSettings::default();
    assert!(values::animations::apply(
        &mut settings,
        "animation-iteration-count",
        "calc(1em / 10px), 2, infinite"
    ));
    let text = settings
        .property_value("animation-iteration-count")
        .unwrap();
    assert!(text.contains("1em"), "{text}");
    let mut roundtrip = values::animations::AnimationSettings::default();
    assert!(values::animations::apply(
        &mut roundtrip,
        "animation-iteration-count",
        &text
    ));
    assert_eq!(roundtrip, settings);
}

#[test]
fn iteration_root_viewport_and_layer_context_are_computed_once() {
    let style = computed(
        "<style>html{font-size:20px}@layer base, top;@layer base{div{animation-iteration-count:calc(1rem / 10px),calc(1vw / 1px),calc(1em / 10px)}}@layer top{div{animation-iteration-count:10;animation-iteration-count:revert-layer;font-size:30px}}</style><div></div>",
        "div",
    );
    assert_eq!(style.animation.iterations, [2.0, 8.0, 3.0]);
}
