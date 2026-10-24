//! GPU-backed transfers are read through native storage, not a stale CPU mirror.
use super::api_version_tests::{call, version_two};
use super::*;
const PACK: i64 = 0x88eb;
const UNPACK: i64 = 0x88ec;
fn buffer(context: &mut WebGl, target: i64, bytes: &[u8]) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(context, "bindBuffer", &[target, id as i64], "");
    context
        .dispatch(
            &Command {
                op: "bufferData".into(),
                i: vec![target, bytes.len() as i64, gl::DYNAMIC_DRAW as i64],
                f: vec![],
                text: String::new(),
            },
            Some(bytes),
        )
        .unwrap();
    id
}
fn invoke(context: &mut WebGl, op: &str, integers: &[i64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: integers.into(),
            f: vec![],
            text: String::new(),
        },
        None,
    )
}
fn contents(context: &mut WebGl, target: i64, size: i64) -> Vec<u8> {
    context
        .read_buffer(&Command {
            op: "getBufferSubData".into(),
            i: vec![target, 0, size],
            f: vec![],
            text: String::new(),
        })
        .unwrap()
}
fn attach(context: &mut WebGl, texture: u32, layer: Option<i64>) {
    let framebuffer = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap();
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, framebuffer as i64],
        "",
    );
    if let Some(layer) = layer {
        call(
            context,
            "framebufferTextureLayer",
            &[
                gl::FRAMEBUFFER as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                texture as i64,
                0,
                layer,
            ],
            "",
        );
    } else {
        call(
            context,
            "framebufferTexture2D",
            &[
                gl::FRAMEBUFFER as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::TEXTURE_2D as i64,
                texture as i64,
                0,
            ],
            "",
        );
    }
}
fn pixels(context: &mut WebGl) -> Vec<u8> {
    context
        .read_pixels(
            &Command {
                op: "readPixels".into(),
                i: vec![0, 0, 2, 2, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 16],
                f: vec![],
                text: String::new(),
            },
            None,
        )
        .unwrap()
}

#[test]
fn webgl2_gpu_pack_then_unpack_uses_actual_buffer_bytes_without_mapping_between_transfers() {
    session::run_native_test(|| {
        let mut context = version_two();
        context
            .dispatch(
                &Command {
                    op: "clearColor".into(),
                    i: vec![],
                    f: vec![1., 0., 0., 1.],
                    text: String::new(),
                },
                None,
            )
            .unwrap();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        let id = buffer(&mut context, PACK, &[0xa5; 20]);
        call(
            &mut context,
            "readPixelsToBuffer",
            &[0, 0, 2, 2, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 4],
            "",
        );
        // No getBufferSubData between write and upload: this must use GPU truth.
        call(&mut context, "bindBuffer", &[UNPACK, id as i64], "");
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap() as u32;
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, texture as i64],
            "",
        );
        call(
            &mut context,
            "texImage2DFromBuffer",
            &[
                gl::TEXTURE_2D as i64,
                0,
                0x8058,
                2,
                2,
                0,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
                4,
            ],
            "",
        );
        call(&mut context, "bindBuffer", &[PACK, 0], "");
        attach(&mut context, texture, None);
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        let actual = contents(&mut context, UNPACK, 20);
        assert_eq!(&actual[..4], &[0xa5; 4]);
        assert!(actual[4..].chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
    });
}

#[test]
fn webgl2_pixel_buffer_zero_offset_is_not_null_allocation_and_padded_ranges_are_bounded() {
    session::run_native_test(|| {
        let mut context = version_two();
        let bytes = [0u8, 255, 0, 255].repeat(4);
        buffer(&mut context, UNPACK, &bytes);
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap() as u32;
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, texture as i64],
            "",
        );
        let args = [
            gl::TEXTURE_2D as i64,
            0,
            0x8058,
            2,
            2,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
            0,
        ];
        invoke(&mut context, "texImage2DFromBuffer", &args).unwrap();
        for offset in [-1, 1, 16, i64::MAX] {
            let mut invalid = args;
            invalid[8] = offset;
            let error = if offset < 0 {
                gl::INVALID_VALUE
            } else {
                gl::INVALID_OPERATION
            };
            assert_eq!(
                invoke(&mut context, "texImage2DFromBuffer", &invalid),
                Err(error)
            );
        }
        attach(&mut context, texture, None);
        assert!(
            pixels(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        call(&mut context, "pixelStorei", &[0x0cf2, 4], "");
        let sub = [
            gl::TEXTURE_2D as i64,
            0,
            0,
            2,
            2,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
            0,
        ];
        assert_eq!(
            invoke(&mut context, "texSubImage2DFromBuffer", &sub),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "bindBuffer", &[UNPACK, 0], "");
        assert_eq!(
            invoke(&mut context, "texImage2DFromBuffer", &args),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_pixel_buffer_offsets_obey_scalar_alignment_and_volume_layer_order() {
    session::run_native_test(|| {
        let mut context = version_two();
        let mut bytes = vec![0xa5; 4];
        bytes.extend([255, 0, 0, 255].repeat(4));
        bytes.extend([0, 0, 255, 255].repeat(4));
        buffer(&mut context, UNPACK, &bytes);
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap() as u32;
        call(&mut context, "bindTexture", &[0x8c1a, texture as i64], "");
        call(
            &mut context,
            "texImage3DFromBuffer",
            &[
                0x8c1a,
                0,
                0x8058,
                2,
                2,
                2,
                0,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
                4,
            ],
            "",
        );
        for (layer, expected) in [(0, [255, 0, 0, 255]), (1, [0, 0, 255, 255])] {
            attach(&mut context, texture, Some(layer));
            assert!(pixels(&mut context).chunks_exact(4).all(|p| p == expected));
        }
        let other = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, other as i64],
            "",
        );
        assert_eq!(
            invoke(
                &mut context,
                "texImage2DFromBuffer",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    0x822e,
                    1,
                    1,
                    0,
                    0x1903,
                    gl::FLOAT as i64,
                    1
                ]
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            invoke(
                &mut context,
                "texImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    0x8058,
                    1,
                    1,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64
                ]
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}

#[test]
fn webgl2_pixel_pack_invalid_offsets_do_not_change_any_native_buffer_byte() {
    session::run_native_test(|| {
        let mut context = version_two();
        buffer(&mut context, PACK, &[0xa5; 16]);
        for (offset, error) in [
            (-1, gl::INVALID_VALUE),
            (1, gl::INVALID_OPERATION),
            (16, gl::INVALID_OPERATION),
            (i64::MAX, gl::INVALID_OPERATION),
        ] {
            assert_eq!(
                invoke(
                    &mut context,
                    "readPixelsToBuffer",
                    &[
                        0,
                        0,
                        2,
                        2,
                        gl::RGBA as i64,
                        gl::UNSIGNED_BYTE as i64,
                        offset
                    ]
                ),
                Err(error)
            );
        }
        assert_eq!(contents(&mut context, PACK, 16), vec![0xa5; 16]);
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x88ed], ""),
            json!(context.core_buffer_bindings[&(PACK as u32)])
        );
    });
}
