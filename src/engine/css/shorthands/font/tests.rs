use super::*;

#[test]
fn retains_authored_units_functions_line_height_and_quoted_family_lists() {
    let parsed = parse("italic 600 calc(1em + 2px) / 1.5 'Family, One', Arial").unwrap();
    assert_eq!(parsed.size, "calc(1em + 2px)");
    assert_eq!(parsed.line_height, "1.5");
    assert_eq!(parsed.family, "'Family, One' , Arial");
    assert_eq!(parsed.style, "italic");
    assert_eq!(parsed.weight, 600);
}

#[test]
fn rejects_duplicate_style_weight_missing_components_and_unimplemented_synthesis() {
    for value in [
        "italic italic 20px Arial",
        "bold 700 20px Arial",
        "400 500 20px Arial",
        "normal normal normal normal normal 20px Arial",
        "small-caps 20px Arial",
        "condensed 20px Arial",
        "caption",
        "20px",
        "20px / Arial",
        "20px / -1 Arial",
        "0px / 1",
        "20px Arial,",
        "20px inherit",
        "1001 20px Arial",
    ] {
        assert!(parse(value).is_none(), "{value}");
    }
}

#[test]
fn style_and_weight_are_order_independent_and_valid_normals_fill_optional_slots() {
    for value in [
        "italic bold 20px Arial",
        "bold italic 20px Arial",
        "normal normal italic bold 20px Arial",
    ] {
        let parsed = parse(value).unwrap();
        assert_eq!(parsed.style, "italic");
        assert_eq!(parsed.weight, 700);
    }
    let parsed = parse("normal normal normal normal 20px Arial").unwrap();
    assert_eq!(parsed.style, "normal");
    assert_eq!(parsed.weight, 400);
}

#[test]
fn cascade_rejects_invalid_shorthand_atomically_without_resetting_existing_features() {
    let mut style = ComputedStyle::initial();
    style.font_features = FontFeatures::parse("'liga' off").unwrap();
    style.font_size = 30.0;
    apply_font_shorthand(&mut style, "bold bold 16px Arial", 30.0, 100.0, 100.0);
    assert_eq!(style.font_size, 30.0);
    assert_eq!(style.font_features.css_text(), "\"liga\" 0");
    apply_font_shorthand(&mut style, "italic 600 2em / 1.5 Arial", 30.0, 100.0, 100.0);
    assert_eq!(style.font_size, 60.0);
    assert!(style.italic);
    assert_eq!(style.font_weight, 600);
    assert_eq!(style.font_features.css_text(), "normal");
}
