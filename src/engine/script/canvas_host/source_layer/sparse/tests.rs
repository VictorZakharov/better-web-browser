use super::*;
use crate::engine::script::canvas_host::composite_layer;

const MODES: &[&str] = &[
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
];

#[test]
fn sparse_rectangle_is_byte_exact_to_dense_for_all_modes_clips_and_alpha_storage() {
    for width in [1, 7, 16] {
        for height in [1, 5, 13] {
            for bounds in [
                [0, 0, width, height],
                [0, 0, 0, 0],
                [width / 2, height / 2, width, height],
                [0, 0, width / 2, height / 2],
                [width / 3, height / 3, width * 2 / 3, height * 2 / 3],
            ] {
                let [left, top, right, bottom] = bounds;
                let mut dense = vec![0; width as usize * height as usize * 4];
                let mut stored = Vec::new();
                for y in top..bottom {
                    for x in left..right {
                        let pixel = [
                            (x * 37 + y * 17) as u8,
                            (x * 19 + y * 13) as u8,
                            (x * 7 + y * 79) as u8,
                            [0, 1, 64, 128, 254, 255][(x + y) as usize % 6],
                        ];
                        dense[(y * width + x) as usize * 4..(y * width + x) as usize * 4 + 4]
                            .copy_from_slice(&pixel);
                        stored.extend(pixel);
                    }
                }
                let layer = Layer {
                    bounds,
                    pixels: stored,
                };
                for opaque in [false, true] {
                    let destination: Vec<_> = (0..width * height)
                        .flat_map(|i| {
                            [
                                (i * 11) as u8,
                                (i * 23) as u8,
                                (i * 43) as u8,
                                if opaque {
                                    255
                                } else {
                                    [0, 1, 63, 127, 255][i as usize % 5]
                                },
                            ]
                        })
                        .collect();
                    for clip in [
                        None,
                        Some(vec![0; (width as usize * height as usize).div_ceil(8)]),
                        Some(vec![
                            0b1010_1100;
                            (width as usize * height as usize).div_ceil(8)
                        ]),
                    ] {
                        for name in MODES {
                            let mode = Operator::parse(name).unwrap();
                            let mut expected = destination.clone();
                            composite_layer::composite_into_with_alpha(
                                &mut expected,
                                &dense,
                                mode,
                                clip.as_deref(),
                                opaque,
                            )
                            .unwrap();
                            let mut actual = destination.clone();
                            composite(
                                &mut actual,
                                width,
                                height,
                                &layer,
                                mode,
                                clip.as_deref(),
                                opaque,
                            )
                            .unwrap();
                            assert_eq!(
                                actual, expected,
                                "{name}/{width}x{height}/{bounds:?}/{opaque}"
                            );
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn invalid_sparse_metadata_never_partially_changes_owned_destination() {
    for layer in [
        Layer {
            bounds: [0, 0, 3, 2],
            pixels: vec![0; 24],
        },
        Layer {
            bounds: [1, 0, 0, 2],
            pixels: Vec::new(),
        },
        Layer {
            bounds: [0, 0, 2, 2],
            pixels: vec![0; 15],
        },
    ] {
        let mut destination = vec![100; 16];
        assert!(
            composite(
                &mut destination,
                2,
                2,
                &layer,
                Operator::SourceOver,
                None,
                false
            )
            .is_none()
        );
        assert_eq!(destination, vec![100; 16]);
    }
    let layer = Layer {
        bounds: [0; 4],
        pixels: Vec::new(),
    };
    let mut destination = vec![100; 16];
    assert!(
        composite(
            &mut destination,
            2,
            2,
            &layer,
            Operator::Copy,
            Some(&[]),
            false
        )
        .is_none()
    );
    assert_eq!(destination, vec![100; 16]);
    assert!(composite(&mut destination, 2, 2, &layer, Operator::Copy, None, true).is_none());
    assert_eq!(destination, vec![100; 16]);
}
