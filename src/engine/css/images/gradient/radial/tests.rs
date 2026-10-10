use super::*;
use crate::engine::css::images::gradient::geometry::Geometry;

fn geometry(value: &str, zoom: f32) -> Geometry {
    Gradient::parse(value)
        .unwrap()
        .resolve(200.0, 100.0, 16.0 * zoom, zoom, Color::BLACK)
        .unwrap()
        .geometry
}

#[test]
fn radial_sizes_accept_shape_order_and_reject_mismatched_or_negative_values() {
    for value in [
        "ellipse 80% 60%",
        "80% 60% ellipse",
        "5em circle",
        "5em",
        "closest-side",
        "farthest-side circle",
        "ellipse at 10% 30%",
    ] {
        assert!(
            Gradient::parse(&format!("radial-gradient({value},red,blue)")).is_some(),
            "{value}"
        );
    }
    for value in [
        "circle 80%",
        "ellipse 5em",
        "circle 2px 4px",
        "-2px",
        "ellipse 30% -40%",
        "circle circle",
        "closest-side farthest-side",
        "10px closest-side",
        "ellipse auto auto",
        "ellipse at",
    ] {
        assert!(
            Gradient::parse(&format!("radial-gradient({value},red,blue)")).is_none(),
            "{value}"
        );
    }
}

#[test]
fn explicit_elliptical_percentages_use_separate_image_dimensions() {
    let Geometry::Radial {
        center,
        radii,
        start_radius,
        end_radius,
    } = geometry("radial-gradient(ellipse 80% 60% at 50% 46%,red,blue)", 1.0)
    else {
        panic!()
    };
    assert_eq!(center, [100.0, 46.0]);
    assert!((radii[0] - 160.0).abs() < 0.0001);
    assert!((radii[1] - 60.0).abs() < 0.0001);
    assert_eq!((start_radius, end_radius), (0.0, 1.0));
    assert_eq!(
        geometry("radial-gradient(ellipse 80% 60% at 50% 46%,red,blue)", 2.0),
        geometry("radial-gradient(ellipse 80% 60% at 50% 46%,red,blue)", 1.0)
    );
    assert_eq!(
        geometry("radial-gradient(10px circle,red,blue)", 2.0),
        Geometry::Radial {
            center: [100.0, 50.0],
            radii: [20.0; 2],
            start_radius: 0.0,
            end_radius: 1.0,
        }
    );
}

#[test]
fn extent_keywords_select_distances_to_infinite_sides_and_corners() {
    for (size, expected) in [
        ("circle closest-side", [20.0; 2]),
        ("circle farthest-side", [180.0; 2]),
        ("ellipse closest-side", [20.0, 30.0]),
        ("ellipse farthest-side", [180.0, 70.0]),
    ] {
        let Geometry::Radial { radii, .. } = geometry(
            &format!("radial-gradient({size} at 20px 30px,red,blue)"),
            1.0,
        ) else {
            panic!()
        };
        assert_eq!(radii, expected, "{size}");
    }
    let Geometry::Radial { radii, .. } = geometry(
        "radial-gradient(ellipse closest-corner at 20px 30px,red,blue)",
        1.0,
    ) else {
        panic!()
    };
    assert_eq!(
        radii,
        [
            20.0 * std::f32::consts::SQRT_2,
            30.0 * std::f32::consts::SQRT_2
        ]
    );
    let Geometry::Radial { radii, .. } = geometry(
        "radial-gradient(circle closest-side at -20px 30px,red,blue)",
        1.0,
    ) else {
        panic!()
    };
    assert_eq!(
        radii, [20.0; 2],
        "sides are infinite lines, not clamped box coordinates"
    );
}

#[test]
fn degenerate_shapes_keep_finite_shader_inputs_and_the_specified_color_limit() {
    let Geometry::Radial { radii, .. } = geometry("radial-gradient(circle 0px,red,blue)", 1.0)
    else {
        panic!()
    };
    assert!(radii.iter().all(|v| v.is_finite() && *v > 0.0));
    assert_eq!(
        geometry("radial-gradient(ellipse 20px 0px,red,blue)", 1.0),
        Geometry::Solid(Color::rgb(0, 0, 255))
    );
    assert_eq!(
        geometry("repeating-radial-gradient(ellipse 20px 0px,red,blue)", 1.0),
        Geometry::Solid(Color::rgb(128, 0, 128))
    );
    assert_eq!(
        geometry("radial-gradient(ellipse calc(-10px) 0px,red,blue)", 1.0),
        Geometry::Radial {
            center: [100.0, 50.0],
            radii: [0.000_001, 1_000_000.0],
            start_radius: 0.0,
            end_radius: 1.0,
        }
    );
}
