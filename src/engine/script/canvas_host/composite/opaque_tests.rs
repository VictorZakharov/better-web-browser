use super::*;

#[test]
fn opaque_porter_duff_output_keeps_premultiplied_rgb_instead_of_replacing_alpha_later() {
    for (mode, expected) in [
        (Operator::Copy, [128, 0, 0, 255]),
        (Operator::SourceIn, [128, 0, 0, 255]),
        (Operator::SourceOut, [0, 0, 0, 255]),
        (Operator::DestinationIn, [0, 0, 128, 255]),
        (Operator::DestinationOut, [0, 0, 127, 255]),
        (Operator::DestinationAtop, [0, 0, 128, 255]),
        (Operator::Xor, [0, 0, 127, 255]),
    ] {
        let mut output = [0, 0, 255, 255];
        mode.pixel_with_alpha(&mut output, [255.0, 0.0, 0.0, 128.0], 1.0, true);
        assert_eq!(output, expected, "{mode:?}");
    }
}

#[test]
fn opaque_compositing_does_not_round_straight_color_before_applying_alpha() {
    for alpha in [0.0, 1.0, 63.0, 128.0, 254.0, 255.0] {
        for opacity in [0.0, 0.2, 0.5, 1.0] {
            let mut output = [10, 20, 30, 255];
            Operator::Copy.pixel_with_alpha(&mut output, [173.0, 91.0, 47.0, alpha], opacity, true);
            let expected =
                [173.0, 91.0, 47.0].map(|value| (value * alpha / 255.0 * opacity).round() as u8);
            assert_eq!(&output[..3], &expected);
            assert_eq!(output[3], 255);
        }
    }
}
