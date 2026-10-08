use super::tests::close;
use super::*;

#[test]
fn rgb_math_uses_component_types_not_suffixes_or_nested_commas() {
    for (source, expected) in [
        (
            "rgb(min(100, 255) calc(10 * 2) max(0, 30))",
            Color::rgb(100, 20, 30),
        ),
        ("rgb(calc(20%) 30 40)", Color::rgb(51, 30, 40)),
        ("rgb(20% calc(30) 40)", Color::rgb(51, 30, 40)),
        (
            "rgb(calc(100%), min(0%, 20%), max(0%, -1%))",
            Color::rgb(255, 0, 0),
        ),
        (
            "rgb(round(254, 10), sqrt(0), mod(256, 256))",
            Color::rgb(250, 0, 0),
        ),
        (
            "rgb(calc(infinity) 0 calc(-infinity))",
            Color::rgb(255, 0, 0),
        ),
        ("rgb(calc(NaN) 20 30)", Color::rgb(0, 20, 30)),
        (
            r"r\67 b(calc(20) min(40, 60) max(40, 60))",
            Color::rgb(20, 40, 60),
        ),
        ("rgb(/**/10/**/ 20/**/ 30/**/)", Color::rgb(10, 20, 30)),
    ] {
        close(parse_color(source).unwrap(), expected, 0);
    }
    for source in [
        "rgb (1 2 3)",
        "rgb/**/(1 2 3)",
        "rgb(calc(20%), 30, 40)",
        "rgb(20%, calc(30), 40)",
        "rgb(min(10px, 20px) 20 30)",
        "rgb(calc(20% + 1px) 20 30)",
        "rgb(calc(20% + 1) 20 30)",
        "rgb(1, 2, none)",
        "rgb(1, 2, 3, none)",
        "rgb(1 2 3 / calc(1s))",
        "rgb(min(1, 2), 3 4)",
        "rgb(1 2 3/**/%)",
    ] {
        assert_eq!(parse_color(source), None, "{source}");
    }
}

#[test]
fn alpha_math_retains_percentage_scale_and_clamps_at_resolution() {
    for source in [
        "rgb(10 20 30 / calc(25%))",
        "rgb(10 20 30 / min(.25, .5))",
        "rgb(10, 20, 30, calc(1 / 4))",
        "rgb(10 20 30/max(10%,25%))",
    ] {
        close(
            parse_color(source).unwrap(),
            Color {
                red: 10,
                green: 20,
                blue: 30,
                alpha: 64,
            },
            0,
        );
    }
    for (source, alpha) in [
        ("rgb(10 20 30 / calc(-1))", 0),
        ("rgb(10 20 30 / calc(infinity))", 255),
        ("rgb(10 20 30 / calc(NaN))", 0),
    ] {
        assert_eq!(parse_color(source).unwrap().alpha, alpha, "{source}");
    }
}

#[test]
fn hue_and_percentage_math_work_across_existing_color_spaces() {
    for source in [
        "hsl(calc(1turn / 3) calc(100%) calc(50%))",
        "hsl(min(120, 240) max(100%, 50%) round(49%, 10%))",
        "hsl(atan2(1, -1) calc(0%) 50%)",
    ] {
        let expected = if source.contains("atan2") {
            Color::rgb(128, 128, 128)
        } else {
            Color::rgb(0, 255, 0)
        };
        close(parse_color(source).unwrap(), expected, 1);
    }
    for (calculated, literal) in [
        (
            "hwb(calc(.5turn) min(20%, 30%) max(10%, 0%))",
            "hwb(180 20% 10%)",
        ),
        (
            "lab(calc(50%) calc(10 + 10) min(30, 40) / calc(.5))",
            "lab(50% 20 30 / .5)",
        ),
        ("lch(calc(50) hypot(3, 4) calc(90deg))", "lch(50 5 90)"),
        ("oklab(calc(.5) calc(.1) calc(-.1))", "oklab(.5 .1 -.1)"),
        ("oklch(calc(.5) calc(.1) calc(.5turn))", "oklch(.5 .1 180)"),
        (
            "color(srgb min(.5, 1) calc(25%) max(0, .75) / calc(.5))",
            "color(srgb .5 .25 .75 / .5)",
        ),
    ] {
        close(
            parse_color(calculated).unwrap(),
            parse_color(literal).unwrap(),
            1,
        );
    }
    for source in [
        "hsl(calc(120%) 100% 50%)",
        "hsl(calc(120ms) 100% 50%)",
        "hsl(0, calc(100), 50%)",
        "hsl(none, 100%, 50%)",
        "hwb(0, 10%, 20%)",
        "lab(min(20, 30), 0, 0)",
        "color(srgb calc(1px) 0 0)",
        "rgb(sign(1em) 0 0)",
    ] {
        assert_eq!(parse_color(source), None, "{source}");
    }
}
