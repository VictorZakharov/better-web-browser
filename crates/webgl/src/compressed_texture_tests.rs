//! Actual ANGLE decoding, not feature-probe-only compressed texture support.
use super::api_version_tests::call;
use super::compressed_capabilities::Family;
use super::core_uniform_tests::program;
use super::*;

pub(super) fn context() -> WebGl {
    WebGl::new(4, 4, Options::default()).unwrap()
}

pub(super) fn enable(context: &mut WebGl, family: Family) {
    let result = call(context, "enableExtension", &[], family.public_name());
    assert_eq!(result, json!(true), "{} unavailable", family.public_name());
}

pub(super) fn texture(context: &mut WebGl, target: u32) -> u32 {
    let id = call(context, "createTexture", &[], "").as_u64().unwrap() as u32;
    call(context, "bindTexture", &[target as i64, id as i64], "");
    for pname in [gl::TEXTURE_MIN_FILTER, gl::TEXTURE_MAG_FILTER] {
        call(
            context,
            "texParameteri",
            &[target as i64, pname as i64, gl::NEAREST as i64],
            "",
        );
    }
    id
}

pub(super) fn upload(
    context: &mut WebGl,
    op: &str,
    integers: &[i64],
    bytes: Option<&[u8]>,
) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: integers.into(),
            f: vec![],
            text: String::new(),
        },
        bytes,
    )
}

pub(super) fn block(format: u32, color: u16) -> Vec<u8> {
    let colors = [color as u8, (color >> 8) as u8, 0, 0, 0, 0, 0, 0];
    match format {
        0x83f0 | 0x83f1 | 0x8c4c | 0x8c4d => colors.to_vec(),
        0x83f2 | 0x8c4e => [vec![255; 8], colors.to_vec()].concat(),
        0x83f3 | 0x8c4f => [vec![255, 0, 0, 0, 0, 0, 0, 0], colors.to_vec()].concat(),
        0x8dbb => vec![255, 0, 0, 0, 0, 0, 0, 0],
        0x8dbc => vec![127, 128, 0, 0, 0, 0, 0, 0],
        0x8dbd => vec![255, 0, 0, 0, 0, 0, 0, 0, 128, 0, 0, 0, 0, 0, 0, 0],
        0x8dbe => vec![127, 128, 0, 0, 0, 0, 0, 0, 64, 128, 0, 0, 0, 0, 0, 0],
        _ => panic!("unsupported fixture format"),
    }
}

pub(super) fn sample(context: &mut WebGl, body: &str, declaration: &str) -> Vec<u8> {
    program(
        context,
        "attribute vec2 p;void main(){gl_Position=vec4(p,0,1);}",
        &format!("precision highp float;{declaration}void main(){{gl_FragColor={body};}}"),
    );
    let buffer = call(context, "createBuffer", &[], "").as_u64().unwrap();
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, buffer as i64],
        "",
    );
    let bytes = [-1.0f32, -1.0, 3.0, -1.0, -1.0, 3.0]
        .into_iter()
        .flat_map(f32::to_ne_bytes)
        .collect::<Vec<_>>();
    upload(
        context,
        "bufferData",
        &[
            gl::ARRAY_BUFFER as i64,
            bytes.len() as i64,
            gl::STATIC_DRAW as i64,
        ],
        Some(&bytes),
    )
    .unwrap();
    // This program has one active attribute; query its real linked location.
    let current = call(context, "getParameter", &[gl::CURRENT_PROGRAM as i64], "")
        .as_u64()
        .unwrap();
    let location = call(context, "getAttribLocation", &[current as i64], "p")
        .as_u64()
        .unwrap();
    call(context, "enableVertexAttribArray", &[location as i64], "");
    call(
        context,
        "vertexAttribPointer",
        &[location as i64, 2, gl::FLOAT as i64, 0, 0, 0],
        "",
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(0));
    context.surface.snapshot().unwrap()
}

#[test]
fn compressed_formats_are_admitted_only_by_their_own_extension() {
    session::run_native_test(|| {
        let mut context = context();
        assert_eq!(call(&mut context, "getParameter", &[0x86a3], ""), json!([]));
        let mut expected = vec![];
        for family in Family::ALL {
            enable(&mut context, family);
            expected.extend(family.formats());
            assert_eq!(
                call(&mut context, "getParameter", &[0x86a3], ""),
                json!(expected)
            );
            enable(&mut context, family);
            assert_eq!(
                call(&mut context, "getParameter", &[0x86a3], ""),
                json!(expected)
            );
        }
    });
}

#[test]
fn compressed_all_twelve_formats_decode_actual_gpu_texels() {
    session::run_native_test(|| {
        let mut context = context();
        texture(&mut context, gl::TEXTURE_2D);
        for family in Family::ALL {
            enable(&mut context, family);
            for format in family.formats() {
                upload(
                    &mut context,
                    "compressedTexImage2D",
                    &[gl::TEXTURE_2D as i64, 0, format as i64, 4, 4, 0],
                    Some(&block(format, 0xf800)),
                )
                .unwrap();
                let pixels = sample(
                    &mut context,
                    "texture2D(t,vec2(0.5))",
                    "uniform sampler2D t;",
                );
                for pixel in pixels.chunks_exact(4) {
                    assert_eq!(pixel[0], 255, "{format:#x}: {pixel:?}");
                    assert_eq!(pixel[2], 0, "{format:#x}: {pixel:?}");
                    assert_eq!(pixel[3], 255, "{format:#x}: {pixel:?}");
                    let green = if format == 0x8dbd || format == 0x8dbe {
                        128
                    } else {
                        0
                    };
                    assert!(
                        (pixel[1] as i32 - green).abs() <= 1,
                        "{format:#x}: {pixel:?}"
                    );
                }
            }
        }
    });
}

#[test]
fn compressed_srgb_storage_decodes_to_linear_before_sampling() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::Srgb);
        texture(&mut context, gl::TEXTURE_2D);
        upload(
            &mut context,
            "compressedTexImage2D",
            &[gl::TEXTURE_2D as i64, 0, 0x8c4d, 4, 4, 0],
            Some(&block(0x8c4d, 0x8410)),
        )
        .unwrap();
        let pixels = sample(
            &mut context,
            "texture2D(t,vec2(0.5))",
            "uniform sampler2D t;",
        );
        for pixel in pixels.chunks_exact(4) {
            assert!((55..=61).contains(&pixel[0]), "{pixel:?}");
            assert!((54..=60).contains(&pixel[1]), "{pixel:?}");
            assert_eq!(pixel[3], 255);
        }
    });
}

#[test]
fn compressed_block_updates_preserve_neighboring_texels() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::S3tc);
        texture(&mut context, gl::TEXTURE_2D);
        let red = block(0x83f1, 0xf800);
        upload(
            &mut context,
            "compressedTexImage2D",
            &[gl::TEXTURE_2D as i64, 0, 0x83f1, 8, 4, 0],
            Some(&[red.clone(), red].concat()),
        )
        .unwrap();
        upload(
            &mut context,
            "compressedTexSubImage2D",
            &[gl::TEXTURE_2D as i64, 0, 4, 0, 4, 4, 0x83f1],
            Some(&block(0x83f1, 0x07e0)),
        )
        .unwrap();
        for (coordinate, expected) in [("0.25", [255, 0, 0, 255]), ("0.75", [0, 255, 0, 255])] {
            let pixels = sample(
                &mut context,
                &format!("texture2D(t,vec2({coordinate},0.5))"),
                "uniform sampler2D t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
    });
}

#[test]
fn compressed_cube_faces_have_independent_native_storage() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::S3tc);
        texture(&mut context, gl::TEXTURE_CUBE_MAP);
        for face in 0..6 {
            let color = if face == 0 { 0xf800 } else { 0x07e0 };
            upload(
                &mut context,
                "compressedTexImage2D",
                &[
                    (gl::TEXTURE_CUBE_MAP_POSITIVE_X + face) as i64,
                    0,
                    0x83f1,
                    4,
                    4,
                    0,
                ],
                Some(&block(0x83f1, color)),
            )
            .unwrap();
        }
        for (direction, expected) in [
            ("vec3(1,0,0)", [255, 0, 0, 255]),
            ("vec3(0,1,0)", [0, 255, 0, 255]),
        ] {
            let pixels = sample(
                &mut context,
                &format!("textureCube(t,{direction})"),
                "uniform samplerCube t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
    });
}

#[test]
fn compressed_alpha_is_decoded_instead_of_forced_opaque() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::S3tc);
        texture(&mut context, gl::TEXTURE_2D);
        let transparent = [0, 0, 0, 0, 255, 255, 255, 255];
        let colors = block(0x83f1, 0xf800);
        let cases = [
            (0x83f1, transparent.to_vec(), [0, 0, 0, 0]),
            (
                0x83f2,
                [vec![0x88; 8], colors.clone()].concat(),
                [255, 0, 0, 136],
            ),
            (
                0x83f3,
                [vec![128, 0, 0, 0, 0, 0, 0, 0], colors].concat(),
                [255, 0, 0, 128],
            ),
        ];
        for (format, bytes, expected) in cases {
            upload(
                &mut context,
                "compressedTexImage2D",
                &[gl::TEXTURE_2D as i64, 0, format, 4, 4, 0],
                Some(&bytes),
            )
            .unwrap();
            let pixels = sample(
                &mut context,
                "texture2D(t,vec2(0.5))",
                "uniform sampler2D t;",
            );
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel == expected),
                "{format:#x}: {pixels:?}"
            );
        }
    });
}

#[test]
fn compressed_signed_rgtc_decodes_negative_values_before_shader_conversion() {
    session::run_native_test(|| {
        let mut context = context();
        enable(&mut context, Family::Rgtc);
        texture(&mut context, gl::TEXTURE_2D);
        for (endpoint, expected) in [(128, 0), (0, 128), (127, 255)] {
            upload(
                &mut context,
                "compressedTexImage2D",
                &[gl::TEXTURE_2D as i64, 0, 0x8dbc, 4, 4, 0],
                Some(&[endpoint, 127, 0, 0, 0, 0, 0, 0]),
            )
            .unwrap();
            let pixels = sample(
                &mut context,
                "vec4(texture2D(t,vec2(0.5)).r*0.5+0.5,0,0,1)",
                "uniform sampler2D t;",
            );
            assert!(
                pixels
                    .chunks_exact(4)
                    .all(|pixel| (pixel[0] as i32 - expected).abs() <= 1 && pixel[3] == 255)
            );
        }
    });
}
