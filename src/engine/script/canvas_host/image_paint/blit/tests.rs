//! Compare admitted exact-center copies to the unchanged general sampler.
use super::super::{render, scalar};
use super::*;

fn request(width: u32, height: u32) -> Request {
    Request {
        width,
        height,
        source_width: width,
        source_height: height,
        bounds: [0, 0, width, height],
        inverse: [1.0, 0.0, 0.0, 1.0, 0.0, 0.0],
        source: [0.0, 0.0, width.into(), height.into()],
        destination: [0.0, 0.0, width.into(), height.into()],
        opacity: 1.0,
        smooth: true,
        operator: "source-over".into(),
    }
}

fn check(request: &Request, destination: &[u8], source: &[u8], clip: Option<&[u8]>) {
    let mut expected = destination.to_vec();
    let mut actual = destination.to_vec();
    scalar::paint(request, &mut expected, source, clip, Operator::SourceOver);
    assert!(paint(
        request,
        &mut actual,
        source,
        clip,
        Operator::SourceOver
    ));
    assert_eq!(
        actual, expected,
        "source {:?}, destination {:?}, inverse {:?}, bounds {:?}",
        request.source, request.destination, request.inverse, request.bounds
    );
    assert_eq!(
        render(request, destination, source, clip).unwrap(),
        expected
    );
}

#[test]
fn every_source_and_backdrop_alpha_matches_with_both_filters() {
    let mut request = request(256, 256);
    let source: Vec<_> = (0..256)
        .flat_map(|alpha| (0..256).flat_map(move |index| [index as u8, 37, 209, alpha as u8]))
        .collect();
    let destination: Vec<_> = (0..256)
        .flat_map(|_| (0..256).flat_map(|alpha| [91, 224, 17, alpha as u8]))
        .collect();
    let source_before = source.clone();
    let destination_before = destination.clone();
    for smooth in [false, true] {
        request.smooth = smooth;
        check(&request, &destination, &source, None);
    }
    assert_eq!(source, source_before);
    assert_eq!(destination, destination_before);
}

#[test]
fn transparent_backdrops_match_every_integer_color_and_source_alpha() {
    let mut request = request(256, 256);
    let source: Vec<_> = (0..=255u8)
        .flat_map(|alpha| {
            (0..=255u8).flat_map(move |channel| [channel, 255 - channel, channel ^ 149, alpha])
        })
        .collect();
    // Hidden RGB is deliberately nonzero. Covered all-transparent pixels must
    // become transparent black; the same bytes outside the clip stay untouched.
    let destination = [71, 219, 103, 0].repeat(256 * 256);
    let clip: Vec<_> = (0..8192)
        .map(|index| [0x55, 0xaa, 0xff, 0][index % 4])
        .collect();
    for smooth in [false, true] {
        request.smooth = smooth;
        for mask in [None, Some(clip.as_slice())] {
            check(&request, &destination, &source, mask);
        }
    }
}

#[test]
fn opaque_rows_copy_only_the_crop_and_leave_padding_untouched() {
    let mut request = request(7, 4);
    request.bounds = [1, 1, 6, 4];
    request.source = [2.0, 0.0, 3.0, 2.0];
    request.destination = [1.0, 1.0, 3.0, 2.0];
    let source: Vec<_> = (0..28).flat_map(|index| [index, 81, 33, 255]).collect();
    let destination = [57; 112];
    check(&request, &destination, &source, None);
    let actual = render(&request, &destination, &source, None).unwrap();
    for row in 0..4 {
        for column in 0..7 {
            let offset = (row * 7 + column) * 4;
            if (1..3).contains(&row) && (1..4).contains(&column) {
                let input = ((row - 1) * 7 + column + 1) * 4;
                assert_eq!(&actual[offset..offset + 4], &source[input..input + 4]);
            } else {
                assert_eq!(&actual[offset..offset + 4], &[57; 4]);
            }
        }
    }
}

#[test]
fn integer_translations_crops_bounds_and_clips_match_the_scalar_oracle() {
    let source: Vec<_> = (0..35)
        .flat_map(|index| {
            [
                (index * 7) as u8,
                (index * 31) as u8,
                43,
                [0, 1, 128, 254, 255][index % 5],
            ]
        })
        .collect();
    let destination: Vec<_> = (0..35)
        .flat_map(|index| [197, (index * 41) as u8, 29, [0, 99, 255][index % 3]])
        .collect();
    let bits = [0b1011_0101; 5];
    let mut random = 0x2c27_7711u32;
    let mut next = |limit: u32| {
        random = random.wrapping_mul(1664525).wrapping_add(1013904223);
        random % limit
    };
    for _ in 0..2048 {
        let mut request = request(7, 5);
        request.source = [
            f64::from(next(13)) - 5.0,
            f64::from(next(9)) - 3.0,
            f64::from(next(8) + 1),
            f64::from(next(6) + 1),
        ];
        request.destination = [
            f64::from(next(13)) - 5.0,
            f64::from(next(9)) - 3.0,
            request.source[2],
            request.source[3],
        ];
        request.inverse[4] = f64::from(next(9)) - 4.0;
        request.inverse[5] = f64::from(next(7)) - 3.0;
        let left = next(8);
        let top = next(6);
        request.bounds = [left, top, left + next(8 - left), top + next(6 - top)];
        for smooth in [false, true] {
            request.smooth = smooth;
            for clip in [None, Some(bits.as_slice())] {
                check(&request, &destination, &source, clip);
            }
        }
    }
}

#[test]
fn empty_intersections_and_fully_masked_copies_do_not_touch_hidden_rgb() {
    let source = [200, 40, 90, 255].repeat(9);
    let destination = [99, 20, 71, 0].repeat(9);
    for value in [-MAX_EXACT_ARGUMENT, -10.0, 10.0, MAX_EXACT_ARGUMENT] {
        let mut request = request(3, 3);
        request.destination[0] = value;
        check(&request, &destination, &source, None);
        assert_eq!(
            render(&request, &destination, &source, None).unwrap(),
            destination
        );
    }
    check(&request(3, 3), &destination, &source, Some(&[0, 0]));
    assert_eq!(
        render(&request(3, 3), &destination, &source, Some(&[0, 0])).unwrap(),
        destination
    );
}

#[test]
fn exact_geometry_boundary_with_large_cancelling_coordinates_is_admitted() {
    let source = [20, 88, 133, 255].repeat(4);
    let destination = [13; 16];
    for boundary in [-MAX_EXACT_ARGUMENT, MAX_EXACT_ARGUMENT] {
        let mut request = request(2, 2);
        request.destination[0] = boundary;
        request.inverse[4] = boundary;
        request.destination[1] = boundary;
        request.inverse[5] = boundary;
        check(&request, &destination, &source, None);
    }
}

#[test]
fn fractional_scaled_rotated_large_and_other_operators_keep_general_sampling() {
    let original = request(3, 3);
    let source = [21, 45, 81, 233].repeat(9);
    let destination = [19; 36];
    let setters: [fn(&mut Request); 10] = [
        |r| r.source[0] = 0.25,
        |r| r.source[1] = -0.5,
        |r| {
            r.source[2] = 1.25;
            r.destination[2] = 1.25;
        },
        |r| r.destination[0] = 0.5,
        |r| r.inverse[4] = 0.1,
        |r| r.inverse[0] = 2.0,
        |r| r.inverse[1] = 0.1,
        |r| r.destination[2] = 2.0,
        |r| r.opacity = 0.5,
        |r| {
            r.destination[0] = MAX_EXACT_ARGUMENT + 1.0;
            r.inverse[4] = r.destination[0];
        },
    ];
    for setter in setters {
        let mut request = request(original.width, original.height);
        setter(&mut request);
        let mut actual = destination;
        assert!(!paint(
            &request,
            &mut actual,
            &source,
            None,
            Operator::SourceOver
        ));
        assert_eq!(actual, destination);
        let mut expected = destination;
        scalar::paint(&request, &mut expected, &source, None, Operator::SourceOver);
        assert_eq!(
            render(&request, &destination, &source, None).unwrap(),
            expected
        );
    }
    for operator in [
        Operator::Copy,
        Operator::DestinationOver,
        Operator::Multiply,
    ] {
        let mut actual = destination;
        assert!(!paint(&original, &mut actual, &source, None, operator));
        assert_eq!(actual, destination);
    }
}
