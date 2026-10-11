use super::*;

#[test]
fn scale_grammar_supports_percentages_reflections_and_2d_z_identity() {
    for (value, expected) in [
        ("50%", [0.5, 0.5]),
        ("-1 2", [-1.0, 2.0]),
        ("2 3 1", [2.0, 3.0]),
        ("calc(2) 100%", [2.0, 1.0]),
    ] {
        assert_eq!(IndividualScale::parse(value).unwrap().0, Some(expected));
    }
    for invalid in ["1px", "1,2", "2 3 2", "NaN", "1 2 3 4"] {
        assert!(IndividualScale::parse(invalid).is_none());
    }
    assert_eq!(IndividualScale::parse("none").unwrap().0, None);
}

#[test]
fn rotate_admits_only_implemented_2d_axes() {
    let a = IndividualRotate::parse("z 45deg").unwrap().0.unwrap();
    assert_eq!(IndividualRotate::parse("0 0 -2 45deg").unwrap().0, Some(-a));
    for invalid in ["x 45deg", "1 0 0 45deg", "20%", "2rad 0", "none 20deg"] {
        assert!(IndividualRotate::parse(invalid).is_none());
    }
}

#[test]
fn individual_linear_interpolation_keeps_negative_values_and_turns() {
    assert_eq!(interpolate_scale("none", "-1 2", 0.5).unwrap(), "0 1.5");
    let value = interpolate_rotate("0deg", "720deg", 0.25).unwrap();
    assert!(
        (IndividualRotate::parse(&value).unwrap().0.unwrap() - std::f32::consts::PI).abs()
            < 0.00001
    );
}

#[test]
fn individual_properties_compose_in_spec_order_not_authored_order() {
    let mut style = ComputedStyle::initial();
    style.translate = individual::IndividualTranslate::parse("10px 20px").unwrap();
    style.rotate = IndividualRotate::parse("180deg").unwrap();
    style.scale = IndividualScale::parse("2 3").unwrap();
    style.transform = parse_transform("translateX(5px)").unwrap();
    let (x, y) = style
        .combined_transform()
        .matrix(100.0, 80.0, 16.0)
        .point(0.0, 0.0);
    assert!(x.abs() < 0.00001 && (y - 20.0).abs() < 0.00001);
    assert!(style.has_transform());
}
