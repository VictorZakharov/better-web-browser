use super::*;

#[test]
fn extreme_used_zoom_products_remain_finite_and_nonzero() {
    use crate::engine::css::values::zoom::combine;
    assert_eq!(combine(2.0, 0.5), 1.0);
    assert_eq!(combine(1.0, 0.0), 1.0);
    let huge = combine(f32::MAX, f32::MAX);
    let tiny = combine(f32::MIN_POSITIVE, f32::MIN_POSITIVE);
    assert!(huge.is_finite() && tiny > 0.0);
    let style = ComputedStyle::initial().zoomed_used_style(huge);
    assert!(style.font_size.is_finite());
}

#[test]
fn zoom_is_non_inherited_and_retains_computed_values() {
    let dom = dom::parse(
        "<style>div{zoom:50%;width:120px;font-size:20px}span{zoom:inherit}b{zoom:0}</style><div><p></p><span></span><b></b></div>",
    );
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let div = dom.elements_named("div").next().unwrap();
    let p = dom.elements_named("p").next().unwrap();
    let span = dom.elements_named("span").next().unwrap();
    let b = dom.elements_named("b").next().unwrap();
    assert_eq!(styles.get(&div).zoom, 0.5);
    assert_eq!(styles.get(&div).width, Length::Px(120.0));
    assert_eq!(styles.get(&p).zoom, 1.0);
    assert_eq!(styles.get(&p).font_size, 20.0);
    assert_eq!(styles.get(&span).zoom, 0.5);
    assert_eq!(
        resolved_property_value(styles.get(&b), "zoom").as_deref(),
        Some("0")
    );
    assert_eq!(super::super::zoom::factor(styles.get(&b).zoom), 1.0);
}

#[test]
fn zoom_admission_rejects_invalid_values_without_discarding_the_cascade() {
    for value in [
        "-1", "-10%", "1px", "auto", "normal", "reset", "1 2", "NaN", "infinity",
    ] {
        assert!(
            !supports::supports_declaration_value("zoom", value),
            "{value}"
        );
    }
    for value in ["0", "1.25", "125%", "calc(1 / 2)", "calc(100% / 2)"] {
        assert!(
            supports::supports_declaration_value("zoom", value),
            "{value}"
        );
    }
    let dom = dom::parse("<div style='zoom:2;zoom:-1'></div>");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    assert_eq!(
        styles.get(&dom.elements_named("div").next().unwrap()).zoom,
        2.0
    );
}

#[test]
fn used_zoom_snapshot_preserves_percentages_and_scales_absolute_calc_terms() {
    let dom =
        dom::parse("<div style='font-size:20px;width:calc(50% - 10px);padding:1em;zoom:2'></div>");
    let styles = StyleSet::from_dom(&dom, &[], 800.0);
    let computed = styles.get(&dom.elements_named("div").next().unwrap());
    let used = computed.zoomed_used_style(2.0);
    assert_eq!(used.width.resolve(200.0, used.font_size), Some(80.0));
    assert_eq!(used.padding.resolve(200.0, used.font_size).top, 40.0);
    assert_eq!(computed.font_size, 20.0);
    assert_eq!(used.font_size, 40.0);
    assert!(!computed.layout_equivalent(&used));
}
