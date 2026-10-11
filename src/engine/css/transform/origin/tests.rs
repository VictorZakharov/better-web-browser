use super::*;

#[test]
fn keyword_origins_use_the_reference_box_not_available_inline_space() {
    for (text, expected) in [
        ("left", (0.0, 40.0)),
        ("top right", (200.0, 0.0)),
        ("center left", (0.0, 40.0)),
        ("bottom", (100.0, 80.0)),
        ("25% 2em", (50.0, 32.0)),
        ("left top 10px", (0.0, 0.0)),
        (r"l\000065ft/**/top", (0.0, 0.0)),
    ] {
        assert_eq!(
            TransformOrigin::parse(text)
                .unwrap_or_else(|| panic!("{text}"))
                .resolve(200.0, 80.0, 16.0),
            expected,
            "{text}"
        );
    }
}

#[test]
fn origin_rejects_edge_offsets_auto_and_percentage_z() {
    for value in [
        "auto",
        "left right",
        "top bottom",
        "left 10px top",
        "left top 10%",
        "top 10px",
        "right 10px bottom 20px",
        "10px, 20px",
    ] {
        assert!(TransformOrigin::parse(value).is_none(), "{value}");
    }
}

#[test]
fn origin_resolves_relative_units_and_zoom_once() {
    let mut origin = TransformOrigin::parse("2rem 10vh").unwrap();
    origin.resolve_relative_units(400.0, 300.0, 20.0);
    origin.apply_zoom(1.25);
    assert_eq!(origin.resolve(100.0, 100.0, 20.0), (50.0, 37.5));
    assert_eq!(TransformBox::default().css_text(), "view-box");
    assert!(TransformBox::Fill.uses_content());
}
