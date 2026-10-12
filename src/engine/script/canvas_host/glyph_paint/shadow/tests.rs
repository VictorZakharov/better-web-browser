use super::*;

fn request(operator: &str, opaque: bool, blur: f64, offset: [f64; 2]) -> ShadowRequest {
    ShadowRequest {
        glyph: Request {
            width: 80,
            height: 64,
            region: [30, 25, 35, 29],
            inverse: [1., 0., 0., 1., 0., 0.],
            scale: 1.,
            opacity: 0.63,
            operator: operator.into(),
            paint: [213.5, 37.25, 61., 179.],
            opaque,
            stroke: false,
        },
        destination: [8, 5, 63, 54],
        blur,
        offset,
        color: [11, 91, 217, 173],
    }
}

fn packet(color: bool) -> Vec<JsValue> {
    let data = if color {
        [201, 33, 11, 128, 0, 255, 111, 255].repeat(10)
    } else {
        [0, 64, 128, 192, 255].repeat(4)
    };
    vec![JsValue::Array(vec![
        JsValue::from(30),
        JsValue::from(25),
        JsValue::from(5),
        JsValue::from(4),
        JsValue::Array([30, 25, 35, 29].map(JsValue::from).into()),
        JsValue::from(color),
        JsValue::Bytes(data),
    ])]
}

fn crop(bytes: &[u8], bounds: [u32; 4], width: u32) -> Vec<u8> {
    let [l, t, r, b] = bounds;
    (t..b)
        .flat_map(|y| {
            bytes[((y * width + l) * 4) as usize..((y * width + r) * 4) as usize]
                .iter()
                .copied()
        })
        .collect()
}

fn dense(
    request: &ShadowRequest,
    destination: &[u8],
    glyphs: &[JsValue],
    clip: Option<&[u8]>,
) -> Vec<u8> {
    let glyph = &request.glyph;
    let source_request = Request {
        width: glyph.width,
        height: glyph.height,
        region: [0, 0, glyph.width, glyph.height],
        inverse: glyph.inverse,
        scale: glyph.scale,
        opacity: glyph.opacity,
        operator: "source-over".into(),
        paint: glyph.paint,
        opaque: false,
        stroke: glyph.stroke,
    };
    let source = render_destination(
        &source_request,
        Cow::Owned(vec![0; destination.len()]),
        glyphs,
        None,
    )
    .unwrap();
    let args = [
        JsValue::Null,
        JsValue::Bytes(destination.to_vec()),
        JsValue::Bytes(source),
        JsValue::from(glyph.operator.clone()),
        clip.map_or(JsValue::Null, |v| JsValue::Bytes(v.to_vec())),
        JsValue::from(glyph.width),
        JsValue::from(glyph.height),
        JsValue::from(request.blur),
        JsValue::from(request.offset[0]),
        JsValue::from(request.offset[1]),
        JsValue::Bytes(request.color.into()),
        JsValue::from(glyph.opaque),
    ];
    super::super::super::source_layer::render(&args, None).unwrap()
}

#[test]
fn compact_shadow_matches_dense_ordered_composition_including_clips_and_color_glyphs() {
    let operators = [
        "source-over",
        "source-atop",
        "destination-over",
        "destination-out",
        "xor",
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
    for opaque in [false, true] {
        for color in [false, true] {
            for operator in operators {
                for (blur, offset) in [(0., [1.5, -2.25]), (2., [-2.5, 3.75]), (8., [0., 1.])] {
                    let r = request(operator, opaque, blur, offset);
                    let destination = [23, 69, 107, if opaque { 255 } else { 113 }].repeat(80 * 64);
                    let glyphs = packet(color);
                    let bits = (0..(80 * 64 / 8))
                        .map(|i| if i % 3 == 0 { 0xa5 } else { 0xff })
                        .collect::<Vec<_>>();
                    let expected = crop(
                        &dense(&r, &destination, &glyphs, Some(&bits)),
                        r.destination,
                        80,
                    );
                    let bytes = crop(&destination, r.destination, 80);
                    let result = render(r, Cow::Borrowed(&bytes), &glyphs, Some(&bits)).unwrap();
                    assert_eq!(
                        result, expected,
                        "{operator}, opaque={opaque}, color={color}, blur={blur}"
                    );
                    assert_eq!(destination[0], 23);
                }
            }
        }
    }
}

#[test]
fn malformed_packet_and_truncated_shadow_never_commit_a_partial_result() {
    let original = [23, 69, 107, 113].repeat(55 * 49);
    for invalid in [JsValue::Null, JsValue::Array(vec![])] {
        let mut glyphs = packet(false);
        glyphs.push(invalid);
        assert!(
            render(
                request("source-over", false, 8., [0., 1.]),
                Cow::Borrowed(&original),
                &glyphs,
                None
            )
            .is_none()
        );
    }
    let mut r = request("source-over", false, 8., [0., 1.]);
    r.destination = r.glyph.region;
    assert!(render(r, Cow::Borrowed(&[0; 80]), &packet(false), None).is_none());
    assert_eq!(original[0], 23);
    for operator in [
        "source-in",
        "source-out",
        "destination-in",
        "destination-atop",
        "copy",
        "unknown",
    ] {
        assert!(
            render(
                request(operator, false, 8., [0., 1.]),
                Cow::Borrowed(&original),
                &packet(false),
                None
            )
            .is_none()
        );
    }
    for (blur, offset) in [
        (f64::NAN, [0., 0.]),
        (-1., [0., 0.]),
        (1., [f64::INFINITY, 0.]),
    ] {
        assert!(
            render(
                request("source-over", false, blur, offset),
                Cow::Borrowed(&original),
                &packet(false),
                None
            )
            .is_none()
        );
    }
}

#[test]
fn owned_and_borrowed_dispatch_agree_without_retaining_or_mutating_author_buffers() {
    let encoded = serde_json::json!({"glyph":{"width":80,"height":64,"region":[30,25,35,29],
        "inverse":[1,0,0,1,0,0],"scale":1,"opacity":0.63,"operator":"source-over",
        "paint":[213.5,37.25,61,179],"opaque":false,"stroke":false},
        "destination":[8,5,63,54],"blur":8,"offset":[0,1],"color":[11,91,217,173]})
    .to_string();
    let args = vec![
        JsValue::Null,
        JsValue::from(encoded),
        JsValue::Bytes([23, 69, 107, 113].repeat(55 * 49)),
        JsValue::Array(packet(false)),
        JsValue::Null,
    ];
    let mut owned = args.clone();
    assert_eq!(paint(&args), paint_owned(&mut owned));
    assert_eq!(owned[2], JsValue::Null);
    assert_eq!(args[2].as_bytes().unwrap()[0], 23);
    assert_eq!(owned[3], args[3]);
}
