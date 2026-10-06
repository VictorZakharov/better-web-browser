use super::*;

#[test]
fn porter_duff_transparent_sources_affect_the_backdrop_with_selected_factors() {
    for (name, expected) in [
        ("source-over", [80, 120, 160, 128]),
        ("source-in", [0; 4]),
        ("source-out", [0; 4]),
        ("source-atop", [80, 120, 160, 128]),
        ("destination-over", [80, 120, 160, 128]),
        ("destination-in", [0; 4]),
        ("destination-out", [80, 120, 160, 128]),
        ("destination-atop", [0; 4]),
        ("xor", [80, 120, 160, 128]),
        ("copy", [0; 4]),
        ("lighter", [80, 120, 160, 128]),
    ] {
        let mut pixel = [80, 120, 160, 128];
        Operator::parse(name)
            .unwrap()
            .pixel(&mut pixel, [200.0, 40.0, 70.0, 0.0], 1.0);
        assert_eq!(pixel, expected, "{name}");
    }
}

#[test]
fn dodge_and_burn_resolve_backdrop_endpoints_before_source_endpoints() {
    assert_eq!(
        blend::color(Operator::ColorDodge, [0.0; 3], [1.0; 3]),
        Some([0.0; 3])
    );
    assert_eq!(
        blend::color(Operator::ColorBurn, [1.0; 3], [0.0; 3]),
        Some([1.0; 3])
    );
    for value in [0.01, 0.25, 0.5, 0.99] {
        assert_eq!(
            blend::color(Operator::ColorDodge, [value; 3], [0.0; 3]),
            Some([value; 3])
        );
        let burn = blend::color(Operator::ColorBurn, [value; 3], [1.0; 3]).unwrap();
        assert!((burn[0] - value).abs() < 1e-15);
    }
}

#[test]
fn all_supported_modes_keep_finite_normalized_color_and_alpha_for_endpoint_grid() {
    for mode in [
        Operator::SourceOver,
        Operator::SourceIn,
        Operator::SourceOut,
        Operator::SourceAtop,
        Operator::DestinationOver,
        Operator::DestinationIn,
        Operator::DestinationOut,
        Operator::DestinationAtop,
        Operator::Xor,
        Operator::Copy,
        Operator::Lighter,
        Operator::Multiply,
        Operator::Screen,
        Operator::Overlay,
        Operator::Darken,
        Operator::Lighten,
        Operator::ColorDodge,
        Operator::ColorBurn,
        Operator::HardLight,
        Operator::SoftLight,
        Operator::Difference,
        Operator::Exclusion,
        Operator::Hue,
        Operator::Saturation,
        Operator::Color,
        Operator::Luminosity,
    ] {
        for source_alpha in [0, 1, 64, 127, 128, 254, 255] {
            for backdrop_alpha in [0, 1, 64, 127, 128, 254, 255] {
                let mut pixel = [0, 128, 255, backdrop_alpha];
                mode.pixel(
                    &mut pixel,
                    [255.0, 128.0, 0.0, f64::from(source_alpha)],
                    1.0,
                );
                if source_alpha == 0 && backdrop_alpha == 0 {
                    assert_eq!(pixel, [0; 4], "{mode:?}");
                }
            }
        }
    }
}

#[test]
fn unrecognized_operator_never_silently_selects_source_over() {
    for name in [
        "",
        "normal",
        "Source-Over",
        "clear",
        "plus-lighter",
        "multiply\0",
    ] {
        assert!(Operator::parse(name).is_none());
    }
}
