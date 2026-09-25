use super::tests::close;
use super::*;

#[test]
fn legacy_and_modern_rgb_syntax_are_not_mixed() {
    for form in [
        "red",
        "#f00",
        "#ff0000",
        "rgb(255, 0, 0)",
        "rgba(255, 0, 0, 1)",
        "rgb(100% 0% 0%)",
        "RGB(255 0 0 / 100%)",
    ] {
        assert_eq!(parse_color(form), Some(Color::rgb(255, 0, 0)), "{form}");
    }
    close(
        parse_color("rgb(10 20 30 / 25%)").unwrap(),
        Color {
            red: 10,
            green: 20,
            blue: 30,
            alpha: 64,
        },
        0,
    );
    for form in [
        "rgb(1, 2 3)",
        "rgb(1, 2, 3 / .5)",
        "rgba(1, 2, 3, .5, 4)",
        "rgb(20% 30 40)",
        "rgb(1 2)",
        "rgb(1 2 3 4)",
        "rgb(1 2 3 /)",
        "rgb(NaN 2 3)",
        "rgb(infinity 2 3)",
        "rgb(1 2 3) junk",
        "rgb()",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn hsl_hue_units_and_legacy_alpha_resolve_to_same_srgb() {
    for form in [
        "hsl(0 100% 50%)",
        "hsl(360deg 100 50)",
        "hsla(0, 100%, 50%, 1)",
        "hsl(400grad 100% 50%)",
        "hsl(6.283185307179586rad 100% 50%)",
        "hsl(1turn 100% 50%)",
    ] {
        assert_eq!(parse_color(form), Some(Color::rgb(255, 0, 0)), "{form}");
    }
    assert_eq!(
        parse_color("hsl(120 100% 50%)"),
        Some(Color::rgb(0, 255, 0))
    );
    assert_eq!(
        parse_color("hsl(240 100% 50%)"),
        Some(Color::rgb(0, 0, 255))
    );
    assert_eq!(
        parse_color("hsl(-120 100% 50%)"),
        Some(Color::rgb(0, 0, 255))
    );
    assert_eq!(
        parse_color("hsl(180 0% 50%)"),
        Some(Color::rgb(128, 128, 128))
    );
    close(
        parse_color("hsl(30 100% 50% / .5)").unwrap(),
        Color {
            red: 255,
            green: 128,
            blue: 0,
            alpha: 128,
        },
        0,
    );
    for form in [
        "hsl(1 2 3 4)",
        "hsl(1, 2, 3 / .5)",
        "hsl(1 2)",
        "hsl(1 2foo 3%)",
        "hsl(1rad 2% 3%, .5)",
    ] {
        assert_eq!(parse_color(form), None, "{form}");
    }
}

#[test]
fn hwb_converts_chromatic_and_achromatic_colors() {
    assert_eq!(parse_color("hwb(0 0% 0%)"), Some(Color::rgb(255, 0, 0)));
    assert_eq!(parse_color("hwb(120 0% 0%)"), Some(Color::rgb(0, 255, 0)));
    assert_eq!(parse_color("hwb(240 0% 0%)"), Some(Color::rgb(0, 0, 255)));
    assert_eq!(parse_color("hwb(45 40% 80%)"), Some(Color::rgb(85, 85, 85)));
    close(
        parse_color("hwb(150 20% 10%)").unwrap(),
        Color::rgb(51, 230, 140),
        1,
    );
    close(
        parse_color("hwb(150 20 10 / 25%)").unwrap(),
        Color {
            red: 51,
            green: 230,
            blue: 140,
            alpha: 64,
        },
        1,
    );
    assert_eq!(parse_color("hwb(0, 0%, 0%)"), None);
}
