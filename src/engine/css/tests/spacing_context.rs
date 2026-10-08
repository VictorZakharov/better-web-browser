//! Spacing inherits a computed length, not an expression using the child's font.
use super::*;

fn computed(markup: &str, tag: &str) -> ComputedStyle {
    let dom = dom::parse(markup);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.get(&dom.elements_named(tag).next().unwrap()).clone()
}

#[test]
fn spacing_uses_final_font_in_either_declaration_order() {
    for font in [10, 20, 30, 40] {
        for order in [true, false] {
            let spacing = "letter-spacing:calc(1em / 5);word-spacing:hypot(.1em, .2em)";
            let declarations = if order {
                format!("font-size:{font}px;{spacing}")
            } else {
                format!("{spacing};font-size:{font}px")
            };
            let style = computed(&format!("<div style='{declarations}'></div>"), "div");
            assert_eq!(style.letter_spacing, font as f32 / 5.0);
            assert!((style.word_spacing - font as f32 * 0.05_f32.sqrt()).abs() < 0.001);
            assert!(!style.letter_spacing_normal);
            assert_eq!(
                style.pending_spacing,
                values::text_spacing::PendingSpacing::default()
            );
        }
    }
}

#[test]
fn inherited_spacing_is_absolute_after_parent_computation() {
    for suffix in [
        "",
        "letter-spacing:inherit;word-spacing:inherit",
        "all:inherit;font-size:10px",
    ] {
        let style = computed(
            &format!(
                "<div style='font-size:30px;letter-spacing:.2em;word-spacing:calc(1em / 10)'><span style='font-size:10px;{suffix}'></span></div>"
            ),
            "span",
        );
        assert_eq!(style.letter_spacing, 6.0);
        assert_eq!(style.word_spacing, 3.0);
        assert!(!style.letter_spacing_normal);
    }
}

#[test]
fn initial_and_literal_spacing_clear_pending_calculations() {
    for (suffix, expected, normal) in [
        ("letter-spacing:normal;word-spacing:normal", 0.0, true),
        ("letter-spacing:0;word-spacing:0", 0.0, false),
        ("letter-spacing:3px;word-spacing:3px", 3.0, false),
        ("letter-spacing:initial;word-spacing:initial", 0.0, true),
        ("all:initial", 0.0, true),
    ] {
        let style = computed(
            &format!(
                "<div style='letter-spacing:2em;word-spacing:3em;{suffix};font-size:30px'></div>"
            ),
            "div",
        );
        assert_eq!(style.letter_spacing, expected);
        assert_eq!(style.word_spacing, expected);
        assert_eq!(style.letter_spacing_normal, normal);
    }
}

#[test]
fn spacing_serializes_computed_zero_as_normal_in_cssom() {
    for (value, expected) in [
        ("normal", "normal"),
        ("0px", "normal"),
        ("mod(12px, 4px)", "normal"),
        ("-2px", "-2px"),
    ] {
        let style = computed(
            &format!("<div style='letter-spacing:{value}'></div>"),
            "div",
        );
        assert_eq!(
            resolved_property_value(&style, "letter-spacing").as_deref(),
            Some(expected)
        );
    }
    let style = computed("<div></div>", "div");
    assert_eq!(
        resolved_property_value(&style, "word-spacing").as_deref(),
        Some("0px")
    );
}

#[test]
fn text3_spacing_rejects_percentages_and_wrong_dimensions() {
    for value in [
        "auto",
        "10%",
        "calc(10% + 2px)",
        "calc(1em / 1px)",
        "1s",
        "1deg",
        "hypot(1px, 1)",
    ] {
        assert!(
            values::text_spacing::SpacingValue::parse(value).is_none(),
            "{value}"
        );
        for property in ["letter-spacing", "word-spacing"] {
            assert!(
                !supports::supports_declaration_value(property, value),
                "{property}:{value}"
            );
            let style = computed(
                &format!("<div style='{property}:2px;{property}:{value}'></div>"),
                "div",
            );
            assert_eq!(
                if property == "letter-spacing" {
                    style.letter_spacing
                } else {
                    style.word_spacing
                },
                2.0
            );
        }
    }
}

#[test]
fn spacing_resolves_root_viewport_and_negative_lengths() {
    let style = computed(
        "<style>html {font-size:20px}</style><div style='font-size:10px;letter-spacing:calc(1rem - .5em);word-spacing:calc(1vw - 1em)'></div>",
        "div",
    );
    assert_eq!(style.letter_spacing, 15.0);
    assert_eq!(style.word_spacing, -2.0);
}

#[test]
fn spacing_revert_layer_restores_the_lower_layer_pending_context() {
    let style = computed(
        "<style>@layer base, top; @layer base {div {letter-spacing:1em;word-spacing:.5em}} @layer top {div {letter-spacing:3em;word-spacing:4em;letter-spacing:revert-layer;word-spacing:revert-layer;font-size:30px}}</style><div></div>",
        "div",
    );
    assert_eq!(style.letter_spacing, 30.0);
    assert_eq!(style.word_spacing, 15.0);
}
