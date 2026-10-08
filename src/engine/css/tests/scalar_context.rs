//! Computed scalar math must use the final cascade, not initial 16px guesses.
use super::*;

fn computed(markup: &str, tag: &str) -> ComputedStyle {
    let dom = dom::parse(markup);
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.get(&dom.elements_named(tag).next().unwrap()).clone()
}

#[test]
fn scalar_context_uses_final_font_independently_of_declaration_order() {
    for font in [10, 20, 30, 40] {
        for order in [true, false] {
            let expressions = "opacity:calc((sign(1em - 20px) + 1) / 2);flex-grow:calc(1em / 10px);flex-shrink:calc(1em / 20px);z-index:calc(1em / 1px);line-height:calc(1em / 10px)";
            let declarations = if order {
                format!("font-size:{font}px;{expressions}")
            } else {
                format!("{expressions};font-size:{font}px")
            };
            let style = computed(&format!("<div style='{declarations}'></div>"), "div");
            assert_eq!(
                style.opacity,
                if font < 20 {
                    0.0
                } else if font == 20 {
                    0.5
                } else {
                    1.0
                }
            );
            assert_eq!(style.flex_grow, font as f32 / 10.0);
            assert_eq!(style.flex_shrink, font as f32 / 20.0);
            assert_eq!(style.z_index, Some(font));
            assert_eq!(style.line_height, font as f32 * font as f32 / 10.0);
            assert_eq!(
                style.scalar_calculations,
                values::scalars::ScalarCalculations::default()
            );
        }
    }
}

#[test]
fn inherited_scalar_values_do_not_reinterpret_parent_font_units() {
    let style = computed(
        "<div style='font-size:30px;opacity:calc(1em / 100px);flex-grow:calc(1em / 10px);flex-shrink:calc(1em / 20px);z-index:calc(1em / 1px);line-height:calc(1em / 10px)'><span style='font-size:10px;opacity:inherit;flex:inherit;z-index:inherit'></span></div>",
        "span",
    );
    assert_eq!(style.opacity, 0.3);
    assert_eq!(style.flex_grow, 3.0);
    assert_eq!(style.flex_shrink, 1.5);
    assert_eq!(style.z_index, Some(30));
    assert_eq!(style.line_height, 30.0);
}

#[test]
fn literal_and_css_wide_overrides_clear_authored_pending_trees() {
    for suffix in [
        "opacity:.7;flex:2 3 10px;z-index:4",
        "opacity:initial;flex:initial;z-index:initial",
        "all:initial",
    ] {
        let style = computed(
            &format!(
                "<div style='opacity:calc(1em / 100px);flex:calc(1em / 10px) calc(1em / 20px) 30px;z-index:calc(1em / 1px);{suffix}'></div>"
            ),
            "div",
        );
        let reset = suffix.contains("initial");
        assert_eq!(style.opacity, if reset { 1.0 } else { 0.7 });
        assert_eq!(style.flex_grow, if reset { 0.0 } else { 2.0 });
        assert_eq!(style.flex_shrink, if reset { 1.0 } else { 3.0 });
        assert_eq!(style.z_index, if reset { None } else { Some(4) });
    }
}

#[test]
fn relative_scalar_ranges_clamp_only_after_resolution() {
    let style = computed(
        "<div style='font-size:30px;opacity:sign(1em);flex-grow:calc(-1em / 10px);flex-shrink:calc(-1em / 20px);line-height:calc(-1em / 10px);z-index:calc(-1em / 20px)'></div>",
        "div",
    );
    assert_eq!(style.opacity, 1.0);
    assert_eq!(style.flex_grow, 0.0);
    assert_eq!(style.flex_shrink, 0.0);
    assert_eq!(style.line_height, 0.0);
    assert_eq!(style.z_index, Some(-1));
}

#[test]
fn font_dependent_z_index_keeps_integer_precision() {
    for (value, expected) in [
        ("16777216 + 1em / 20px", 16_777_217),
        ("16777217 + 1em / 40px", 16_777_218),
        ("-16777218 + 1em / 40px", -16_777_217),
    ] {
        let style = computed(
            &format!("<div style='font-size:20px;z-index:calc({value})'></div>"),
            "div",
        );
        assert_eq!(style.z_index, Some(expected), "{value}");
    }
}

#[test]
fn type_admission_does_not_invent_a_percentage_basis() {
    for property in [
        "opacity",
        "flex-grow",
        "flex-shrink",
        "line-height",
        "z-index",
    ] {
        for source in ["sign(1em)", "calc(1vw / 1px)", "calc(1rem / 1px)"] {
            assert!(
                supports::supports_declaration_value(property, source),
                "{property}:{source}"
            );
        }
        for source in [
            "calc(1em)",
            "calc(1em + 2)",
            "calc(1em / 1s)",
            "calc(1em / 1px) junk",
        ] {
            if property == "line-height" && source == "calc(1em)" {
                continue;
            }
            assert!(
                !supports::supports_declaration_value(property, source),
                "{property}:{source}"
            );
        }
    }
}

#[test]
fn root_units_use_document_root_not_current_element_font() {
    let style = computed(
        "<html style='font-size:30px'><body><div style='font-size:10px;opacity:calc(1rem / 100px);flex-grow:calc(1rem / 10px);line-height:calc(1rem / 10px)'></div></body></html>",
        "div",
    );
    assert_eq!(style.opacity, 0.3);
    assert_eq!(style.flex_grow, 3.0);
    assert_eq!(style.line_height, 30.0);
}

#[test]
fn viewport_dimensions_are_available_to_number_valued_consumers() {
    for (width, height) in [(800.0, 600.0), (1200.0, 300.0), (320.0, 1000.0)] {
        let dom = dom::parse(
            "<div style='font-size:10px;opacity:calc(1vw / 100px);flex-grow:calc(1vh / 1px);flex-shrink:calc(1vmin / 1px);z-index:calc(1vmax / 1px);line-height:calc(1vw / 1px);aspect-ratio:calc(1vw / 1px) / calc(1vh / 1px)'></div>",
        );
        let styles =
            StyleSet::from_sources_for_viewport(&dom, "https://example.test/", &[], width, height);
        let style = styles.get(&dom.elements_named("div").next().unwrap());
        assert!((style.opacity - width / 10_000.0).abs() < 0.00001);
        assert_eq!(style.flex_grow, height / 100.0);
        assert_eq!(style.flex_shrink, width.min(height) / 100.0);
        assert_eq!(
            style.z_index,
            Some(((width.max(height) / 100.0) + 0.5).floor() as i32)
        );
        assert_eq!(style.line_height, width / 10.0);
        assert_eq!(
            style.aspect_ratio,
            AspectRatio::Ratio {
                width: width / 100.0,
                height: height / 100.0,
                prefer_natural: false
            }
        );
    }
}

#[test]
fn ratio_calculations_keep_both_components_and_natural_preference() {
    for (source, width, height, prefer_natural) in [
        ("calc(1em / 10px)", 3.0, 1.0, false),
        ("calc(1em / 10px) / calc(1em / 20px)", 3.0, 1.5, false),
        ("auto calc(1em / 10px) / 2", 3.0, 2.0, true),
        ("calc(1em / 10px) / 2 auto", 3.0, 2.0, true),
        ("calc(-1em / 10px) / 2", 0.0, 2.0, false),
        ("2 / calc(-1em / 10px)", 2.0, 0.0, false),
    ] {
        let style = computed(
            &format!("<div style='aspect-ratio:{source};font-size:30px'></div>"),
            "div",
        );
        assert_eq!(
            style.aspect_ratio,
            AspectRatio::Ratio {
                width,
                height,
                prefer_natural
            },
            "{source}"
        );
    }
    for suffix in [
        "aspect-ratio:2 / 3",
        "aspect-ratio:initial",
        "aspect-ratio:auto",
    ] {
        let style = computed(
            &format!("<div style='aspect-ratio:calc(1em / 10px) / 2;{suffix}'></div>"),
            "div",
        );
        assert_eq!(
            style.aspect_ratio,
            if suffix.contains("2 / 3") {
                AspectRatio::Ratio {
                    width: 2.0,
                    height: 3.0,
                    prefer_natural: false,
                }
            } else {
                AspectRatio::Auto
            }
        );
    }
}

#[test]
fn invalid_replacements_preserve_or_reset_pending_values_at_the_correct_stage() {
    for (suffix, expected) in [
        ("opacity:calc(1em + 2)", 0.3),
        ("--bad:calc(1em + 2);opacity:var(--bad)", 1.0),
    ] {
        let style = computed(
            &format!("<div style='font-size:30px;opacity:calc(1em / 100px);{suffix}'></div>"),
            "div",
        );
        assert_eq!(style.opacity, expected);
    }
}

#[test]
fn cascade_layers_restore_pending_math_before_final_font_resolution() {
    let style = computed(
        "<style>@layer base,top; @layer base{div{opacity:calc(1em / 100px);flex:calc(1em / 10px) calc(1em / 20px) 20px;aspect-ratio:calc(1em / 10px) / 2;z-index:calc(1em / 1px)}} @layer top{div{opacity:.9;opacity:revert-layer;flex:5 6 70px;flex:revert-layer;aspect-ratio:4 / 5;aspect-ratio:revert-layer;z-index:7;z-index:revert-layer;font-size:30px}}</style><div></div>",
        "div",
    );
    assert_eq!(style.opacity, 0.3);
    assert_eq!(style.flex_grow, 3.0);
    assert_eq!(style.flex_shrink, 1.5);
    assert_eq!(style.flex_basis, Length::Px(20.0));
    assert_eq!(style.z_index, Some(30));
    assert_eq!(
        style.aspect_ratio,
        AspectRatio::Ratio {
            width: 3.0,
            height: 2.0,
            prefer_natural: false
        }
    );
}
