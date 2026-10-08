//! Shared filter grammar and assignment-time relative-value contracts.
use super::*;

fn scalar(source: &str, environment: Environment) -> f32 {
    let operations = parse(source, environment).unwrap_or_else(|| panic!("invalid {source}"));
    assert_eq!(operations.len(), 1);
    let Argument::Scalar(value) = operations[0].value else {
        panic!("not scalar")
    };
    value
}

#[test]
fn tokenizer_handles_case_comments_exponents_escapes_and_adjacent_functions() {
    for source in [
        "/**/BrIgHtNeSs(1e-1)/**/",
        r"b\72 ightness(1e-1)",
        "brightness(/*amount*/.1)",
    ] {
        assert!(
            (scalar(source, Environment::default()) - 0.1).abs() < 1e-6,
            "{source}"
        );
    }
    let values = parse("brightness(50%)contrast(2)", Environment::default()).unwrap();
    assert_eq!(
        values.iter().map(|value| value.name).collect::<Vec<_>>(),
        vec!["brightness", "contrast"]
    );
    assert!(
        parse("NONE/**/", Environment::default())
            .unwrap()
            .is_empty()
    );
    assert!(
        parse(r"n\6f ne", Environment::default())
            .unwrap()
            .is_empty()
    );
}

#[test]
fn defaults_clamps_and_typed_math_share_the_css_consumers() {
    for name in [
        "brightness",
        "contrast",
        "grayscale",
        "invert",
        "opacity",
        "saturate",
        "sepia",
    ] {
        assert_eq!(scalar(&format!("{name}()"), Environment::default()), 1.0);
        assert_eq!(
            scalar(&format!("{name}(calc(2 - 3))"), Environment::default()),
            0.0
        );
        let expected = if matches!(name, "brightness" | "contrast" | "saturate") {
            2.0
        } else {
            1.0
        };
        assert_eq!(
            scalar(&format!("{name}(200%)"), Environment::default()),
            expected
        );
    }
    assert_eq!(scalar("blur()", Environment::default()), 0.0);
    assert_eq!(scalar("blur(/* default */)", Environment::default()), 0.0);
    assert_eq!(
        scalar("brightness(/* default */)", Environment::default()),
        1.0
    );
    assert_eq!(scalar("blur(calc(1px - 2px))", Environment::default()), 0.0);
    assert_eq!(scalar("hue-rotate()", Environment::default()), 0.0);
    assert_eq!(scalar("hue-rotate(-0)", Environment::default()), 0.0);
    for source in [
        "hue-rotate(200grad)",
        "hue-rotate(.5turn)",
        "hue-rotate(calc(90deg + 90deg))",
    ] {
        assert!((scalar(source, Environment::default()) - std::f32::consts::PI).abs() < 1e-5);
    }
}

#[test]
fn absolute_font_root_and_viewport_lengths_use_their_actual_bases() {
    let environment = Environment {
        font_size: 20.0,
        root_font_size: 30.0,
        viewport_width: 1000.0,
        viewport_height: 500.0,
        ..Environment::default()
    };
    for (source, expected) in [
        ("blur(1in)", 96.0),
        ("blur(72pt)", 96.0),
        ("blur(6pc)", 96.0),
        ("blur(2.54cm)", 96.0),
        ("blur(25.4mm)", 96.0),
        ("blur(101.6q)", 96.0),
        ("blur(.5em)", 10.0),
        ("blur(.5rem)", 15.0),
        ("blur(1vw)", 10.0),
        ("blur(1vh)", 5.0),
        ("blur(1vmin)", 5.0),
        ("blur(1vmax)", 10.0),
        ("blur(calc(1em + 1rem + 1vw))", 60.0),
    ] {
        assert!(
            (scalar(source, environment) - expected).abs() < 1e-4,
            "{source}"
        );
    }
}

#[test]
fn shadow_offsets_are_a_contiguous_group_and_current_color_is_resolved() {
    let environment = Environment {
        current_color: Color::rgb(31, 71, 113),
        ..Environment::default()
    };
    for source in [
        "drop-shadow(-2px 3px)",
        "drop-shadow(currentColor -2px 3px 0)",
        "drop-shadow(-2px 3px CURRENTCOLOR)",
    ] {
        let operations = parse(source, environment).unwrap();
        let Argument::Shadow {
            x,
            y,
            blur,
            channels,
            origin_clean,
        } = operations[0].value
        else {
            panic!()
        };
        assert_eq!((x, y, blur, channels), (-2.0, 3.0, 0.0, [31, 71, 113, 255]));
        assert!(!origin_clean);
    }
    for source in [
        "drop-shadow(rgb(31 71 113 / .5) 2px 3px)",
        "drop-shadow(2px 3px #1f477180)",
    ] {
        let operations = parse(source, environment).unwrap();
        let Argument::Shadow { channels, .. } = operations[0].value else {
            panic!()
        };
        assert_eq!(channels, [31, 71, 113, 128]);
    }
}

#[test]
fn malformed_unsupported_and_wrongly_typed_values_do_not_return_a_partial_list() {
    for source in [
        "",
        " ",
        "blur(-1px)",
        "brightness(-.1)",
        "brightness(inf)",
        "brightness(1e400)",
        "brightness(1px)",
        "blur(1%)",
        "blur(calc(0%))",
        "blur(calc(1px + 0%))",
        "blur(auto)",
        "blur(1 px)",
        "blur (1px)",
        "hue-rotate(1)",
        "hue-rotate(calc(0))",
        "blur(1px) nope(1)",
        "none blur(1px)",
        "url(#filter)",
        "drop-shadow(1px)",
        "drop-shadow(1px red 2px)",
        "drop-shadow(1px 2px -1px)",
        "drop-shadow(red 1px 2px blue)",
        "drop-shadow(1px 2px 3px 4px)",
        "drop-shadow(1% 2px)",
        "drop-shadow(1px 2px red !important)",
    ] {
        assert!(parse(source, Environment::default()).is_none(), "{source}");
    }
}

#[test]
fn storage_function_count_and_nested_math_are_bounded() {
    assert!(parse(&"blur(0) ".repeat(64), Environment::default()).is_some());
    assert!(parse(&"blur(0) ".repeat(65), Environment::default()).is_none());
    assert!(
        parse(
            &format!("brightness(/*{}*/1)", "x".repeat(16_384)),
            Environment::default()
        )
        .is_none()
    );
    assert!(
        parse(
            &format!("blur({}1px{})", "calc(".repeat(64), ")".repeat(64)),
            Environment::default()
        )
        .is_none()
    );
}
