use super::*;

#[test]
fn individual_translate_has_strict_2d_admission_and_none_is_not_zero() {
    for value in [
        "none",
        "0",
        "-50% -50%",
        "calc(2em - 3px) 25%",
        "2px 3px 0px",
    ] {
        assert!(IndividualTranslate::parse(value).is_some(), "{value}");
    }
    for value in [
        "",
        "auto",
        "10",
        "1px,2px",
        "1px 2px 3px",
        "1px 2px 0%",
        "1px 2px calc(1em - 16px)",
        "1px 2px 0px 0px",
    ] {
        assert!(IndividualTranslate::parse(value).is_none(), "{value}");
    }
    let mut style = ComputedStyle::initial();
    assert!(!style.has_transform());
    style.translate = IndividualTranslate::parse("0").unwrap();
    assert!(
        style.has_transform(),
        "identity still establishes a containing block"
    );
}

#[test]
fn translation_composes_with_transform_and_zoom_without_double_scaling_percentages() {
    let mut style = ComputedStyle::initial();
    style.translate = IndividualTranslate::parse("-50% 2em").unwrap();
    style.transform = parse_transform("translate(10px, 5px)").unwrap();
    let used = style.zoomed_used_style(2.0);
    assert_eq!(
        used.combined_transform()
            .resolve(200.0, 100.0, used.font_size),
        (-80.0, 74.0)
    );
    assert_eq!(style.translate.css_text(), "-50% 2em");
}

#[test]
fn individual_translate_interpolation_keeps_length_bases_deferred() {
    let value = interpolate("none", "50% 2em", 0.5).unwrap();
    let result = IndividualTranslate::parse(&value).unwrap();
    let mut style = ComputedStyle::initial();
    style.translate = result;
    assert_eq!(
        style.combined_transform().resolve(200.0, 100.0, 20.0),
        (50.0, 20.0)
    );
    assert_eq!(interpolate("none", "none", 0.5).as_deref(), Some("none"));
    assert!(interpolate("0", "10px", f64::NAN).is_none());
}
