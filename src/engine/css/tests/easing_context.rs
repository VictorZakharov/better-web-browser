//! Deferred linear outputs share the same final scalar context as lengths.
use super::*;

fn computed(declarations: &str) -> ComputedStyle {
    let dom = dom::parse(&format!("<div style='{declarations}'></div>"));
    let styles =
        StyleSet::from_sources_for_viewport(&dom, "https://example.test/", &[], 800.0, 600.0);
    styles
        .get(&dom.elements_named("div").next().unwrap())
        .clone()
}

#[test]
fn linear_outputs_use_final_font_and_preserve_input_positions() {
    for declarations in [
        "font-size:40px;animation-timing-function:linear(0,calc(1em / 20px) 50% 70%,1);transition-timing-function:linear(0,calc(1em / 20px) 50% 70%,1)",
        "animation-timing-function:linear(0,50% 70% calc(1em / 20px),1);transition-timing-function:linear(0,50% 70% calc(1em / 20px),1);font-size:40px",
    ] {
        let style = computed(declarations);
        assert_eq!(style.animation.easings, ["linear(0, 2 50% 70%, 1)"]);
        assert_eq!(style.transition.easings, ["linear(0, 2 50% 70%, 1)"]);
        assert!(style.animation.calculated_easings.is_empty());
        assert!(style.transition.calculated_easings.is_empty());
    }
}

#[test]
fn linear_lists_resolve_font_and_viewport_without_losing_boundaries() {
    let style = computed(
        "font-size:40px;animation-timing-function:linear(0,calc(1em / 20px)),ease,linear(0,calc(1vw / 1px));transition-timing-function:linear(0,calc(1vh / 1px)),linear(0,calc(-1em / 20px))",
    );
    assert_eq!(
        style.animation.easings,
        ["linear(0, 2)", "ease", "linear(0, 8)"]
    );
    assert_eq!(style.transition.easings, ["linear(0, 6)", "linear(0, -2)"]);
}

#[test]
fn contextual_easing_admission_keeps_number_and_percentage_types_separate() {
    for property in ["animation-timing-function", "transition-timing-function"] {
        for value in [
            "linear(0,sign(1em))",
            "linear(0,calc(1vw / 1px))",
            "linear(0,50% 80% calc(1em / 10px),1)",
        ] {
            assert!(
                supports::supports_declaration_value(property, value),
                "{property}:{value}"
            );
        }
        for value in [
            "linear(0,calc(1em))",
            "linear(0,calc(1s))",
            "linear(0,sign(1%))",
            "linear(0,50% calc(1em / 10px) 80%,1)",
            "linear(0,calc(1em / 10px) 10% 20% 30%)",
            "cubic-bezier(sign(1em),0,1,1)",
            "steps(sign(1em))",
        ] {
            assert!(
                !supports::supports_declaration_value(property, value),
                "{property}:{value}"
            );
        }
    }
}

#[test]
fn invalid_list_replacements_do_not_erase_pending_context() {
    let style = computed(
        "font-size:40px;animation-timing-function:linear(0,calc(1em / 20px)),ease;animation-timing-function:linear(0,calc(1em / 20px)),broken;transition-timing-function:linear(0,calc(1em / 20px)),ease;transition-timing-function:linear(0,calc(1em / 20px)),broken",
    );
    assert_eq!(style.animation.easings, ["linear(0, 2)", "ease"]);
    assert_eq!(style.transition.easings, ["linear(0, 2)", "ease"]);
}

#[test]
fn literal_easing_overrides_clear_authored_pending_outputs() {
    for suffix in [
        "animation-timing-function:ease;transition-timing-function:ease",
        "animation:initial;transition:initial",
    ] {
        let style = computed(&format!(
            "font-size:40px;animation-timing-function:linear(0,calc(1em / 20px));transition-timing-function:linear(0,calc(1em / 20px));{suffix}"
        ));
        assert_eq!(style.animation.easings, ["ease"]);
        assert_eq!(style.transition.easings, ["ease"]);
    }
}

#[test]
fn cssom_authored_easing_does_not_serialize_the_placeholder() {
    let source = "linear(0,calc(1em / 20px))";
    let specified = cssom::declarations::value("animation-timing-function", source).unwrap();
    assert!(specified.contains("1em"), "{specified}");
    let style = computed(&format!(
        "animation-timing-function:{specified};font-size:40px"
    ));
    assert_eq!(style.animation.easings, ["linear(0, 2)"]);
}
