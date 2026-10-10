use super::*;
use gradient::geometry::Geometry;

fn resolved(value: &str) -> gradient::geometry::ResolvedGradient {
    Gradient::parse(value)
        .unwrap()
        .resolve(100.0, 100.0, 16.0, 1.0, Color::BLACK)
        .unwrap()
}

#[test]
fn image_layers_preserve_order_resolve_urls_and_serialize() {
    let images = parse(
        "linear-gradient(red, blue), url('../pic.png'), none",
        "https://example.test/css/main.css",
        false,
    )
    .unwrap();
    assert!(matches!(images.0[0], ImageSource::Gradient(_)));
    assert_eq!(images.as_deref(), Some("https://example.test/pic.png"));
    assert_eq!(
        images.css_text(),
        "linear-gradient(red, blue), url(\"https://example.test/pic.png\"), none"
    );
}

#[test]
fn invalid_images_fail_atomically_and_bounds_are_enforced() {
    for value in [
        "",
        "url(a) url(b)",
        "none none",
        "linear-gradient(red)",
        "linear-gradient(to left left, red, blue)",
        "radial-gradient(circle ellipse, red, blue)",
        "radial-gradient(circle at, red, blue)",
        "linear-gradient(red auto, blue)",
        "conic-gradient(red,blue)",
        "linear-gradient(red,blue), garbage",
    ] {
        assert!(
            parse(value, "https://example.test/", false).is_none(),
            "{value}"
        );
    }
    assert!(
        parse(
            &std::iter::repeat_n("none", 33)
                .collect::<Vec<_>>()
                .join(","),
            "",
            false
        )
        .is_none()
    );
    assert!(parse(&" ".repeat(16_385), "", false).is_none());
}

#[test]
fn direction_angles_and_omitted_stops_resolve_to_the_css_gradient_line() {
    let value = resolved("linear-gradient(90deg, red, white, blue)");
    let Geometry::Linear { start, end } = value.geometry else {
        panic!()
    };
    assert!(start[0].abs() < 0.01 && (end[0] - 100.0).abs() < 0.01);
    assert!((start[1] - 50.0).abs() < 0.01);
    assert_eq!(
        value.stops.iter().map(|s| s.0).collect::<Vec<_>>(),
        [0.0, 0.5, 1.0]
    );
}

#[test]
fn out_of_range_linear_stops_adjust_endpoints_without_clamping_colors() {
    let value = resolved("linear-gradient(to right, red -50%, blue 150%)");
    assert_eq!(
        value.geometry,
        Geometry::Linear {
            start: [-50.0, 50.0],
            end: [150.0, 50.0]
        }
    );
    assert_eq!(
        value.stops.iter().map(|s| s.0).collect::<Vec<_>>(),
        [0.0, 1.0]
    );
}

#[test]
fn descending_positions_and_double_position_stops_fix_up_without_reordering() {
    let value = resolved("linear-gradient(to right, red 20% 40%, white 10%, blue 80%)");
    assert_eq!(value.stops.len(), 4);
    assert!((value.stops[1].0 - 1.0 / 3.0).abs() < 0.001);
    assert_eq!(value.stops[1].0, value.stops[2].0);
}

#[test]
fn zero_period_repeating_gradient_uses_premultiplied_average() {
    let value = resolved("repeating-linear-gradient(red 0px, white 0px, blue 0px)");
    assert_eq!(value.geometry, Geometry::Solid(Color::rgb(191, 128, 191)));
    let value = resolved("repeating-linear-gradient(transparent 0px, red 0px)");
    assert_eq!(
        value.geometry,
        Geometry::Solid(Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128
        })
    );
}

#[test]
fn relative_stops_use_root_viewport_and_zoom_without_scaling_percentages_twice() {
    let mut gradient =
        Gradient::parse("linear-gradient(to right, currentcolor 1rem, blue 10vw)").unwrap();
    gradient.resolve_relative_units(800.0, 600.0, 20.0);
    let value = gradient
        .resolve(100.0, 100.0, 32.0, 2.0, Color::WHITE)
        .unwrap();
    assert_eq!(
        value.geometry,
        Geometry::Linear {
            start: [40.0, 50.0],
            end: [160.0, 50.0]
        }
    );
    assert_eq!(value.stops[0].1, Color::WHITE);
}

#[test]
fn radial_stop_extent_and_negative_prefix_do_not_clamp_to_the_original_radius() {
    let value = resolved("radial-gradient(circle, red -50%, blue 50%)");
    let Geometry::Radial {
        start_radius,
        end_radius,
        ..
    } = value.geometry
    else {
        panic!()
    };
    assert_eq!((start_radius, end_radius), (0.0, 0.5));
    assert_eq!(value.stops[0], (0.0, Color::rgb(128, 0, 128)));
    let value = resolved("repeating-radial-gradient(circle, red -50%, blue 50%)");
    let Geometry::Radial {
        start_radius,
        end_radius,
        ..
    } = value.geometry
    else {
        panic!()
    };
    assert_eq!((start_radius, end_radius), (0.5, 1.5));
}
