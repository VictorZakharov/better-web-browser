use super::*;

#[test]
fn opaque_host_flag_is_closed_and_rejects_nonopaque_backdrops_atomically() {
    let mut args = vec![
        JsValue::Null,
        JsValue::Bytes(vec![0, 0, 255, 255]),
        JsValue::Bytes(vec![255, 0, 0, 128]),
        JsValue::from("copy".to_string()),
        JsValue::Null,
        JsValue::Boolean(true),
    ];
    assert_eq!(paint(&args), JsValue::Bytes(vec![128, 0, 0, 255]));
    let original = args.clone();
    for invalid in [
        JsValue::Null,
        JsValue::from(1.0),
        JsValue::from("true".to_string()),
    ] {
        args[5] = invalid;
        assert_eq!(paint(&args), JsValue::Null);
        assert_eq!(&args[..5], &original[..5]);
    }
    args = original;
    args[1] = JsValue::Bytes(vec![10, 20, 30, 254]);
    let input = args.clone();
    assert_eq!(paint(&args), JsValue::Null);
    assert_eq!(args, input);
}

#[test]
fn opaque_whole_surface_copy_clears_uncovered_pixels_to_black_and_honors_clip() {
    let mut pixels = [10, 20, 30, 255].repeat(4);
    let source = [255, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    composite_into_with_alpha(&mut pixels, &source, Operator::Copy, Some(&[0b0111]), true).unwrap();
    assert_eq!(
        pixels,
        [128, 0, 0, 255, 0, 0, 0, 255, 0, 0, 0, 255, 10, 20, 30, 255]
    );
}

#[test]
fn zero_source_copy_clears_all_unclipped_pixels_without_changing_owned_inputs() {
    let destination = [13, 70, 200, 128].repeat(9);
    let source = [0; 36];
    let output = composite(
        &destination,
        &source,
        Operator::Copy,
        Some(&[0b0101_0101, 1]),
    )
    .unwrap();
    for (index, pixel) in output.chunks_exact(4).enumerate() {
        assert_eq!(
            pixel,
            if index % 2 == 0 {
                &[0; 4]
            } else {
                &[13, 70, 200, 128]
            }
        );
    }
    assert_eq!(destination, [13, 70, 200, 128].repeat(9));
    assert_eq!(source, [0; 36]);
}

#[test]
fn full_surface_source_in_and_destination_in_include_outside_shape() {
    let destination = [0, 0, 255, 128, 0, 255, 0, 255];
    let source = [255, 0, 0, 128, 0, 0, 0, 0];
    assert_eq!(
        composite(&destination, &source, Operator::SourceIn, None).unwrap(),
        [255, 0, 0, 64, 0, 0, 0, 0]
    );
    assert_eq!(
        composite(&destination, &source, Operator::DestinationIn, None).unwrap(),
        [0, 0, 255, 64, 0, 0, 0, 0]
    );
}

#[test]
fn invalid_lengths_and_clip_payloads_decline_before_any_drawing() {
    for (destination, source, clip) in [
        (&[][..], &[][..], None),
        (&[0; 3][..], &[0; 3][..], None),
        (&[0; 4][..], &[0; 8][..], None),
        (&[0; 4][..], &[0; 4][..], Some(&[][..])),
        (&[0; 4][..], &[0; 4][..], Some(&[0; 2][..])),
    ] {
        assert!(composite(destination, source, Operator::Copy, clip).is_none());
    }
    assert!(
        composite(
            &vec![0; (MAX_CANVAS_PIXELS + 1) * 4],
            &[],
            Operator::Copy,
            None
        )
        .is_none()
    );
    assert_eq!(paint(&[]), JsValue::Null);
}

#[test]
fn fully_closed_clip_preserves_hidden_rgb_and_repeated_outputs_do_not_alias() {
    let destination = [41, 59, 70, 0];
    let mut output = composite(&destination, &[255; 4], Operator::Copy, Some(&[0])).unwrap();
    assert_eq!(output, destination);
    output.fill(255);
    assert_eq!(
        composite(&destination, &[255; 4], Operator::Copy, Some(&[0])).unwrap(),
        destination
    );
}

#[test]
fn transparent_fast_skip_matches_scalar_equation_including_hidden_backdrop_rgb() {
    for name in [
        "source-over",
        "source-in",
        "source-out",
        "source-atop",
        "destination-over",
        "destination-in",
        "destination-out",
        "destination-atop",
        "xor",
        "copy",
        "lighter",
        "multiply",
        "screen",
        "overlay",
        "darken",
        "lighten",
        "color-dodge",
        "color-burn",
        "hard-light",
        "soft-light",
        "difference",
        "exclusion",
        "hue",
        "saturation",
        "color",
        "luminosity",
    ] {
        let mode = Operator::parse(name).unwrap();
        for alpha in [0, 1, 127, 255] {
            for destination in [[41, 59, 70, alpha], [0, 0, 0, alpha]] {
                for source in [[201, 123, 37, 0], [0, 0, 0, 0]] {
                    let mut expected = destination;
                    mode.pixel(&mut expected, source.map(f64::from), 1.0);
                    assert_eq!(
                        composite(&destination, &source, mode, None).unwrap(),
                        expected,
                        "{name}/{alpha}/{destination:?}/{source:?}"
                    );
                }
            }
        }
    }
}
