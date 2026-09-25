use super::*;
use crate::engine::css::{StyleSet, supports::supports_matches};
use crate::engine::dom;

pub(super) fn close(actual: Color, expected: Color, tolerance: u8) {
    for (actual, expected) in [
        (actual.red, expected.red),
        (actual.green, expected.green),
        (actual.blue, expected.blue),
        (actual.alpha, expected.alpha),
    ] {
        assert!(
            actual.abs_diff(expected) <= tolerance,
            "expected {expected}±{tolerance}, got {actual}"
        );
    }
}

#[test]
fn predefined_color_spaces_are_converted_before_srgb8_output() {
    for form in [
        "color(srgb 1 0 0)",
        "color(srgb-linear 1 0 0)",
        "color(display-p3 1 0 0)",
        "color(display-p3-linear 1 0 0)",
    ] {
        assert_eq!(parse_color(form), Some(Color::rgb(255, 0, 0)), "{form}");
    }
    for form in [
        "color(a98-rgb 0 0 0)",
        "color(prophoto-rgb 0 0 0)",
        "color(rec2020 0 0 0)",
    ] {
        assert_eq!(parse_color(form), Some(Color::BLACK), "{form}");
    }
    for form in [
        "color(a98-rgb 1 1 1)",
        "color(prophoto-rgb 1 1 1)",
        "color(rec2020 1 1 1)",
    ] {
        close(parse_color(form).unwrap(), Color::WHITE, 1);
    }
    // Mid-gray distinguishes these spaces' transfer curves from sRGB and
    // catches accidentally treating wide-gamut channel values as sRGB.
    close(
        parse_color("color(a98-rgb .5 .5 .5)").unwrap(),
        Color::rgb(129, 129, 129),
        2,
    );
    close(
        parse_color("color(prophoto-rgb .5 .5 .5)").unwrap(),
        Color::rgb(146, 146, 146),
        3,
    );
    close(
        parse_color("color(rec2020 .5 .5 .5)").unwrap(),
        Color::rgb(120, 120, 120),
        3,
    );
    close(
        parse_color("color(rec2020 .42053 .979780 .00579)").unwrap(),
        Color::rgb(0, 255, 0),
        1,
    );
    close(
        parse_color("color(srgb-linear .5 .5 .5)").unwrap(),
        Color::rgb(188, 188, 188),
        0,
    );
    close(
        parse_color("color(xyz-d50 .9643 1 .8251)").unwrap(),
        Color::WHITE,
        1,
    );
    close(
        parse_color("color(xyz-d65 .9505 1 1.089)").unwrap(),
        Color::WHITE,
        1,
    );
    close(
        parse_color("color(xyz-d50 .2005 .14089 .4472)").unwrap(),
        Color::rgb(118, 84, 205),
        2,
    );
    close(
        parse_color("color(srgb .25 .5 .75 / .4)").unwrap(),
        Color {
            red: 64,
            green: 128,
            blue: 191,
            alpha: 102,
        },
        0,
    );
    for form in [
        "color(display-p3 1 0)",
        "color(no-such-space 1 0 0)",
        "color(srgb 1 0 0 1)",
        "color(srgb 1 0 0 / 0 / 1)",
        "color(display-p3 NaN 0 0)",
        "color(profoto-rgb 1 0 0)",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn perceptual_colors_use_d50_or_d65_as_specified() {
    for form in [
        "lab(100% 0 0)",
        "lch(100% 0 0)",
        "oklab(100% 0 0)",
        "oklch(100% 0 0)",
    ] {
        close(parse_color(form).unwrap(), Color::WHITE, 1);
    }
    for form in [
        "lab(0% 0 0)",
        "lch(0% 0 0)",
        "oklab(0% 0 0)",
        "oklch(0% 0 0)",
    ] {
        close(parse_color(form).unwrap(), Color::BLACK, 1);
    }
    close(
        parse_color("lab(44.36% 36.05 -58.99)").unwrap(),
        Color::rgb(118, 84, 205),
        3,
    );
    close(
        parse_color("lch(44.36% 69.13 301.44)").unwrap(),
        Color::rgb(118, 84, 205),
        4,
    );
    close(
        parse_color("oklab(.5 0 0 / 50%)").unwrap(),
        Color {
            red: 99,
            green: 99,
            blue: 99,
            alpha: 128,
        },
        2,
    );
    close(
        parse_color("oklch(.5 0 none / .5)").unwrap(),
        Color {
            red: 99,
            green: 99,
            blue: 99,
            alpha: 128,
        },
        2,
    );
    for form in [
        "lab(10% 20)",
        "lab(10%, 20, 30)",
        "oklch(50% 0.2 bogus)",
        "oklab(Infinity 0 0)",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn color_mix_normalizes_weights_and_premultiplies_alpha() {
    assert_eq!(
        parse_color("color-mix(in srgb, red, blue)"),
        Some(Color::rgb(128, 0, 128))
    );
    assert_eq!(
        parse_color("color-mix(in SRGB, red 25%, blue)"),
        Some(Color::rgb(64, 0, 191))
    );
    assert_eq!(
        parse_color("color-mix(in srgb-linear, red, blue)"),
        Some(Color::rgb(188, 0, 188))
    );
    close(
        parse_color("color-mix(in oklab, white, black)").unwrap(),
        Color::rgb(99, 99, 99),
        2,
    );
    close(
        parse_color("color-mix(white, black)").unwrap(),
        Color::rgb(99, 99, 99),
        2,
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red 30%, blue 30%)"),
        Some(Color {
            red: 128,
            green: 0,
            blue: 128,
            alpha: 153
        })
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red 80%, blue 80%)"),
        Some(Color::rgb(128, 0, 128))
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red 0%, blue 0%)"),
        Some(Color {
            red: 128,
            green: 0,
            blue: 128,
            alpha: 0
        })
    );
    assert_eq!(
        parse_color("color-mix(in srgb, transparent, red)"),
        Some(Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 128
        })
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red, green, blue)"),
        Some(Color::rgb(85, 43, 85))
    );
    close(
        parse_color("color-mix(in srgb, rgb(255, 0, 0) 50%, color-mix(in srgb, blue, blue))")
            .unwrap(),
        Color::rgb(128, 0, 128),
        0,
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red)"),
        Some(Color::rgb(255, 0, 0))
    );
    assert_eq!(
        parse_color("color-mix(in srgb, red 30%)"),
        Some(Color {
            red: 255,
            green: 0,
            blue: 0,
            alpha: 77
        })
    );
    for form in [
        "color-mix(in unsupported, red, blue)",
        "color-mix(in srgb, red -1%, blue)",
        "color-mix(in srgb, red 101%, blue)",
        "color-mix(in srgb, red,)",
        "color-mix(in srgb, rgb(1, 2, 3, blue)",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn color_mix_interpolates_in_each_rectangular_css_color_space() {
    // Mixing identical colors exercises both directions of each color-space
    // conversion without relying on an sRGB-only implementation shortcut.
    for space in [
        "srgb",
        "srgb-linear",
        "oklab",
        "lab",
        "xyz",
        "xyz-d65",
        "xyz-d50",
        "display-p3",
        "display-p3-linear",
        "a98-rgb",
        "prophoto-rgb",
        "rec2020",
    ] {
        let input = format!("color-mix(in {space}, rgb(37 106 212), rgb(37 106 212))");
        close(parse_color(&input).unwrap(), Color::rgb(37, 106, 212), 1);
    }
    assert_eq!(
        parse_color("color-mix(in xyz-d65, black, white)"),
        Some(Color::rgb(188, 188, 188))
    );
    close(
        parse_color("color-mix(in xyz-d50, black, white)").unwrap(),
        Color::rgb(188, 188, 188),
        1,
    );
    close(
        parse_color("color-mix(in lab, black, white)").unwrap(),
        Color::rgb(119, 119, 119),
        2,
    );
    close(
        parse_color("color-mix(in rec2020, black, white)").unwrap(),
        Color::rgb(120, 120, 120),
        2,
    );
    // Predefined wide-gamut source channels are not rounded to sRGB8 before
    // interpolation; only the final paint result is clipped and quantized.
    for space in ["display-p3", "a98-rgb", "prophoto-rgb", "rec2020"] {
        let input =
            format!("color-mix(in {space}, color({space} .25 .5 .75), color({space} .25 .5 .75))");
        let direct = parse_color(&format!("color({space} .25 .5 .75)")).unwrap();
        close(parse_color(&input).unwrap(), direct, 1);
    }
}

#[test]
fn color_mix_follows_hue_arcs_in_cylindrical_spaces() {
    assert_eq!(
        parse_color("color-mix(in hsl, red, blue)"),
        Some(Color::rgb(255, 0, 255))
    );
    assert_eq!(
        parse_color("color-mix(in hsl longer hue, red, blue)"),
        Some(Color::rgb(0, 255, 0))
    );
    assert_eq!(
        parse_color("color-mix(in hsl increasing hue, red, blue)"),
        Some(Color::rgb(0, 255, 0))
    );
    assert_eq!(
        parse_color("color-mix(in hsl decreasing hue, red, blue)"),
        Some(Color::rgb(255, 0, 255))
    );
    assert_eq!(
        parse_color("color-mix(in hwb, red, blue)"),
        Some(Color::rgb(255, 0, 255))
    );
    assert_eq!(
        parse_color("color-mix(in hwb longer hue, red, blue)"),
        Some(Color::rgb(0, 255, 0))
    );
    close(
        parse_color("color-mix(in lch, peru 40%, palegoldenrod)").unwrap(),
        parse_color("lch(79.7256% 40.448 84.771)").unwrap(),
        3,
    );
    for space in ["hsl", "hwb", "lch", "oklch"] {
        let input = format!("color-mix(in {space}, rgb(37 106 212), rgb(37 106 212))");
        close(parse_color(&input).unwrap(), Color::rgb(37, 106, 212), 2);
    }
    // Powerless hue in gray inherits the other color's hue rather than
    // producing NaN or interpolating against an arbitrary zero-degree hue.
    let neutral = parse_color("color-mix(in hsl, gray, blue)").unwrap();
    assert!(neutral.blue > neutral.red && neutral.blue > neutral.green);
    for form in [
        "color-mix(in srgb longer hue, red, blue)",
        "color-mix(in hsl backwards hue, red, blue)",
        "color-mix(in hsl longer, red, blue)",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn color_functions_feed_the_real_cascade_and_supports_query() {
    for color in [
        "hsl(120 100% 50%)",
        "hwb(120 0% 0%)",
        "color(srgb 0 1 0)",
        "color-mix(in srgb, lime, lime)",
    ] {
        assert!(
            supports_matches(&format!("@supports (color: {color})")),
            "{color}"
        );
        let dom = dom::parse(&format!("<style>p{{color:{color}}}</style><p>green</p>"));
        let node = dom.elements_named("p").next().unwrap();
        let styles = StyleSet::from_dom(&dom, &[], 1000.0);
        assert_eq!(styles.get(&node).color, Color::rgb(0, 255, 0), "{color}");
    }
    assert!(!supports_matches(
        "@supports (color: color(no-such-space 1 0 0))"
    ));
}
