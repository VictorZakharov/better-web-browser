use super::*;

fn resolved(css: &str, declarations: &[(&str, &str)]) -> Vec<(String, String)> {
    let dom = dom::parse(&format!(
        "<style>{css}</style><main><div id=target></div></main>"
    ));
    let target = dom.elements_named("div").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    styles.resolved_keyframe_values(
        &target,
        &declarations
            .iter()
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect::<Vec<_>>(),
    )
}

#[test]
fn keyframes_resolve_custom_properties_and_current_font_relative_units() {
    let values = resolved(
        "#target{--end:.8;font-size:20px}",
        &[("opacity", "var(--end)"), ("width", "2em")],
    );
    assert_eq!(
        values,
        [
            ("opacity".into(), "0.8".into()),
            ("width".into(), "40px".into())
        ]
    );
}

#[test]
fn inherit_uses_actual_parent_instead_of_target_presentation() {
    let values = resolved(
        "main{color:blue} #target{color:red}",
        &[("color", "inherit")],
    );
    assert_eq!(values, [("color".into(), "rgb(0, 0, 255)".into())]);
}

#[test]
fn box_shorthands_expand_to_independently_interpolable_longhands() {
    let values = resolved("", &[("margin", "1px 2px 3px 4px"), ("padding", "5px 6px")]);
    assert_eq!(
        values,
        [
            ("margin-top".into(), "1px".into()),
            ("margin-right".into(), "2px".into()),
            ("margin-bottom".into(), "3px".into()),
            ("margin-left".into(), "4px".into()),
            ("padding-top".into(), "5px".into()),
            ("padding-right".into(), "6px".into()),
            ("padding-bottom".into(), "5px".into()),
            ("padding-left".into(), "6px".into()),
        ]
    );
}

#[test]
fn malformed_and_control_properties_do_not_enter_keyframe_values() {
    let values = resolved(
        "",
        &[
            ("opacity", "garbage"),
            ("animation-name", "fade"),
            ("transition-duration", "1s"),
            ("not-supported", "10px"),
            ("opacity", ".4"),
        ],
    );
    assert_eq!(
        values.iter().filter(|(name, _)| name == "opacity").count(),
        1
    );
    assert!(!values.iter().any(|(name, _)| name == "animation-name"));
    assert_eq!(
        values.iter().find(|(name, _)| name == "opacity").unwrap().1,
        "0.4"
    );
}

#[test]
fn font_size_resolves_before_lengths_regardless_of_declaration_order() {
    let values = resolved("", &[("width", "2em"), ("font-size", "20px")]);
    assert_eq!(
        values.iter().find(|(name, _)| name == "width").unwrap().1,
        "40px"
    );
}

#[test]
fn native_underlying_query_excludes_own_effect_but_keeps_parent_inheritance() {
    let dom = dom::parse("<main><div></div></main>");
    let parent = dom.elements_named("main").next().unwrap();
    let target = dom.elements_named("div").next().unwrap();
    parent.set_animation_style("color:blue");
    target.set_animation_style("color:red;opacity:.9");
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(styles.get(&target).color, Color::rgb(255, 0, 0));
    let underlying = styles.underlying_style_for_node(&target);
    assert_eq!(underlying.color, Color::rgb(0, 0, 255));
    assert_eq!(underlying.opacity, 1.0);
    assert_eq!(styles.get(&target).opacity, 0.9);
}

#[test]
fn budgets_reject_oversized_values_and_bound_result_properties() {
    let dom = dom::parse("<div></div>");
    let target = dom.elements_named("div").next().unwrap();
    let mut styles = StyleSet::from_dom(&dom, &[], 800.0);
    let declarations = vec![("opacity".into(), " ".repeat(2048) + ".5")];
    assert!(
        styles
            .resolved_keyframe_values(&target, &declarations)
            .is_empty()
    );
}
