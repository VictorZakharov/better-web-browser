use super::*;

#[test]
fn serializes_supported_flex_values() {
    let mut style = ComputedStyle::initial();
    style.flex_direction = FlexDirection::Column;
    style.flex_grow = 2.5;
    style.flex_shrink = 0.75;
    style.flex_wrap = true;

    assert_eq!(
        resolved_property_value(&style, "flex-direction").as_deref(),
        Some("column")
    );
    assert_eq!(
        resolved_property_value(&style, "flex-grow").as_deref(),
        Some("2.5")
    );
    assert_eq!(
        resolved_property_value(&style, "flex-shrink").as_deref(),
        Some("0.75")
    );
    assert_eq!(
        resolved_property_value(&style, "flex-wrap").as_deref(),
        Some("wrap")
    );
}

#[test]
fn serializes_computed_box_values_used_by_transitions() {
    let mut style = ComputedStyle::initial();
    style.border_radius = Length::Px(8.0);
    style.margin.top = Length::Px(3.0);
    style.margin.right = Length::Percent(10.0);
    style.margin.bottom = Length::Px(5.0);
    style.margin.left = Length::Auto;

    assert_eq!(
        resolved_property_value(&style, "border-radius").as_deref(),
        Some("8px")
    );
    assert_eq!(
        resolved_property_value(&style, "margin-top").as_deref(),
        Some("3px")
    );
    assert_eq!(
        resolved_property_value(&style, "margin-right").as_deref(),
        Some("10%")
    );
    assert_eq!(
        resolved_property_value(&style, "margin-bottom").as_deref(),
        Some("5px")
    );
    assert_eq!(
        resolved_property_value(&style, "margin-left").as_deref(),
        Some("auto")
    );
}

#[test]
fn serializes_computed_float_keywords() {
    let mut style = ComputedStyle::initial();
    style.float = Float::Right;

    assert_eq!(
        resolved_property_value(&style, "float").as_deref(),
        Some("right")
    );
}

#[test]
fn serializes_integer_and_auto_z_index() {
    let mut style = ComputedStyle::initial();
    assert_eq!(
        resolved_property_value(&style, "z-index").as_deref(),
        Some("auto")
    );

    style.z_index = Some(-7);
    assert_eq!(
        resolved_property_value(&style, "z-index").as_deref(),
        Some("-7")
    );
}

#[test]
fn resolves_inherited_case_sensitive_custom_properties_for_cssom() {
    let mut style = ComputedStyle::initial();
    Arc::make_mut(&mut style.custom_properties).insert("--Accent".into(), "rgb(1, 2, 3)".into());
    Arc::make_mut(&mut style.custom_properties).insert("--alias".into(), "var(--Accent)".into());

    assert_eq!(
        resolved_property_value(&style, "--Accent").as_deref(),
        Some("rgb(1, 2, 3)")
    );
    assert_eq!(
        resolved_property_value(&style, "--alias").as_deref(),
        Some("rgb(1, 2, 3)")
    );
    assert_eq!(resolved_property_value(&style, "--accent"), None);
}
