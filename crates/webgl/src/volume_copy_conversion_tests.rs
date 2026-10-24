//! Non-renderable copies preserve actual HDR/integer pixels and author state.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::texture_targets::VOLUME;
use super::volume_copy_tests::{clear, sample};
use super::*;

fn volume(context: &mut WebGl, format: u32) {
    texture(context, VOLUME);
    call(
        context,
        "texStorage3D",
        &[VOLUME as i64, 1, format as i64, 4, 4, 3],
        "",
    );
}

fn copy(context: &mut WebGl, source: [i64; 4], offsets: [i64; 3]) -> Result<Value> {
    upload(
        context,
        "copyTexSubImage3D",
        &[
            VOLUME as i64,
            0,
            offsets[0],
            offsets[1],
            offsets[2],
            source[0],
            source[1],
            source[2],
            source[3],
        ],
        None,
    )
}

fn shader_check(context: &mut WebGl, sampler: &str, expression: &str) {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    super::core_uniform_tests::program(
        context,
        super::core_uniform_tests::VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;uniform highp {sampler} t;out vec4 color;void main(){{color=({expression})?vec4(0,1,0,1):vec4(1,0,0,1);}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    let pixels = context.surface.snapshot().unwrap();
    assert!(
        pixels.chunks_exact(4).all(|p| p == [0, 255, 0, 255]),
        "{expression}: {pixels:?}"
    );
}

#[test]
fn webgl2_volume_copy_preserves_hdr_half_and_float_without_normalized_clamping() {
    session::run_native_test(|| {
        for (destination, source) in [(0x881b, 0x881a), (0x8815, 0x8814)] {
            let mut context = version_two();
            call(
                &mut context,
                "enableExtension",
                &[],
                "EXT_color_buffer_float",
            );
            volume(&mut context, destination);
            super::array_copy_boundary_tests::framebuffer(&mut context, source);
            context
                .dispatch(
                    &Command {
                        op: "clearBufferfv".into(),
                        i: vec![0x1800, 0],
                        f: vec![2.5, -1.0, 0.25, 1.0],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            copy(&mut context, [0, 0, 4, 4], [0, 0, 1]).unwrap();
            shader_check(
                &mut context,
                "sampler3D",
                "all(equal(textureLod(t,vec3(0.5),0.0).rgb,vec3(2.5,-1.0,0.25))) && all(equal(textureLod(t,vec3(0.5,0.5,0.1),0.0).rgb,vec3(0))) && all(equal(textureLod(t,vec3(0.5,0.5,0.9),0.0).rgb,vec3(0)))",
            );
        }
    });
}

#[test]
fn webgl2_volume_copy_preserves_all_rgb_integer_widths_and_signedness() {
    session::run_native_test(|| {
        for (destination, source, signed, values) in [
            (0x8d8f, 0x8d8e, true, [-32, 17, -1]),
            (0x8d89, 0x8d88, true, [-32000, 17000, -1]),
            (0x8d83, 0x8d82, true, [-2147483648, 123456789, -1]),
            (0x8d7d, 0x8d7c, false, [255, 17, 129]),
            (0x8d77, 0x8d76, false, [65535, 17000, 32769]),
            (0x8d71, 0x8d70, false, [4294967295, 123456789, 2147483649]),
        ] {
            let mut context = version_two();
            volume(&mut context, destination);
            super::array_copy_boundary_tests::framebuffer(&mut context, source);
            let mut args = vec![0x1800, 0];
            args.extend(values);
            args.push(1);
            upload(
                &mut context,
                if signed {
                    "clearBufferiv"
                } else {
                    "clearBufferuiv"
                },
                &args,
                None,
            )
            .unwrap();
            copy(&mut context, [0, 0, 4, 4], [0, 0, 1]).unwrap();
            let vector = if signed { "ivec3" } else { "uvec3" };
            let literal = values
                .map(|value| {
                    if signed {
                        format!("{value}")
                    } else {
                        format!("{value}u")
                    }
                })
                .join(",");
            shader_check(
                &mut context,
                if signed { "isampler3D" } else { "usampler3D" },
                &format!(
                    "all(equal(textureLod(t,vec3(0.5),0.0).rgb,{vector}({literal}))) && all(equal(textureLod(t,vec3(0.5,0.5,0.1),0.0).rgb,{vector}(0))) && all(equal(textureLod(t,vec3(0.5,0.5,0.9),0.0).rgb,{vector}(0)))"
                ),
            );
        }
    });
}

#[test]
fn webgl2_volume_copy_clips_reads_and_preserves_texels_outside_the_intersection() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(
            &mut context,
            "enableExtension",
            &[],
            "EXT_color_buffer_float",
        );
        volume(&mut context, 0x8815);
        let source = super::array_copy_boundary_tests::framebuffer(&mut context, 0x8814);
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        copy(&mut context, [0, 0, 4, 4], [0, 0, 1]).unwrap();
        clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        copy(&mut context, [-2, 0, 4, 4], [0, 0, 1]).unwrap();
        for region in [[-10, 0, 4, 4], [4, 0, 4, 4], [0, 4, 4, 4]] {
            copy(&mut context, region, [0, 0, 1]).unwrap();
        }
        for (uv, expected) in [
            ("vec3(0.25,0.5,0.5)", [255, 0, 0, 255]),
            ("vec3(0.75,0.5,0.5)", [0, 255, 0, 255]),
            ("vec3(0.75,0.5,0.1)", [0, 0, 0, 255]),
        ] {
            let pixels = sample(&mut context, VOLUME, uv, 0);
            assert!(
                pixels.chunks_exact(4).all(|p| p == expected),
                "{uv}: {pixels:?}"
            );
        }
        call(
            &mut context,
            "bindFramebuffer",
            &[super::framebuffer_guard::READ as i64, source as i64],
            "",
        );
        let charged = context.resource_bytes;
        let budget = context.resource_limit;
        context.resource_limit = charged;
        assert_eq!(
            copy(&mut context, [0, 0, 4, 4], [0, 0, 1]),
            Err(gl::OUT_OF_MEMORY)
        );
        context.resource_limit = budget;
        assert_eq!(context.resource_bytes, charged);
        let pixels = sample(&mut context, VOLUME, "vec3(0.75,0.5,0.5)", 0);
        assert!(pixels.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    });
}

#[test]
fn webgl2_volume_copy_private_read_ignores_pbos_and_restores_pack_unpack_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(
            &mut context,
            "enableExtension",
            &[],
            "EXT_color_buffer_float",
        );
        volume(&mut context, 0x8815);
        let source = super::array_copy_boundary_tests::framebuffer(&mut context, 0x8814);
        clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        call(
            &mut context,
            "bindFramebuffer",
            &[super::framebuffer_guard::DRAW as i64, 0],
            "",
        );
        let mut buffers = Vec::new();
        for target in [0x88eb, 0x88ec] {
            let id = call(&mut context, "createBuffer", &[], "")
                .as_u64()
                .unwrap();
            call(&mut context, "bindBuffer", &[target, id as i64], "");
            call(
                &mut context,
                "bufferData",
                &[target, 1, gl::STATIC_DRAW as i64],
                "",
            );
            buffers.push(id);
        }
        let stores = [
            (gl::PACK_ALIGNMENT as i64, 8),
            (gl::UNPACK_ALIGNMENT as i64, 8),
            (0x0d02, 17),
            (0x0d03, 3),
            (0x0d04, 2),
            (0x0cf2, 19),
            (0x0cf3, 4),
            (0x0cf4, 3),
            (0x806e, 21),
            (0x806d, 2),
        ];
        for (pname, value) in stores {
            call(&mut context, "pixelStorei", &[pname, value], "");
        }
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[0, 0, 0, 0], "");
        copy(&mut context, [0, 0, 4, 4], [0, 0, 1]).unwrap();
        for (pname, value) in stores {
            assert_eq!(
                call(&mut context, "getParameter", &[pname], ""),
                json!(value)
            );
            let mut actual = 0;
            unsafe { gl::GetIntegerv(pname as u32, &mut actual) };
            assert_eq!(actual, value as i32);
        }
        for (pname, id) in [(0x88ed, buffers[0]), (0x88ef, buffers[1])] {
            assert_eq!(call(&mut context, "getParameter", &[pname], ""), json!(id));
            let native = context.objects.get(id as u32, Kind::Buffer).unwrap().native;
            let mut actual = 0;
            unsafe { gl::GetIntegerv(pname as u32, &mut actual) };
            assert_eq!(actual as u32, native);
        }
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(source)
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
            json!(true)
        );
        call(&mut context, "disable", &[gl::SCISSOR_TEST as i64], "");
        let pixels = sample(&mut context, VOLUME, "vec3(0.5)", 0);
        assert!(pixels.chunks_exact(4).all(|p| p == [0, 255, 0, 255]));
    });
}

#[test]
fn webgl2_volume_copy_native_conversion_preserves_rgb_float_neighbor_layers() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = texture(&mut context, VOLUME);
        call(
            &mut context,
            "texStorage3D",
            &[VOLUME as i64, 1, 0x8815, 4, 4, 3],
            "",
        );
        let values: [f32; 3] = [0.0, 0.0, 0.5];
        let bytes: Vec<u8> = (0..48)
            .flat_map(|_| values.iter().flat_map(|value| value.to_ne_bytes()))
            .collect();
        upload(
            &mut context,
            "texSubImage3D",
            &[
                VOLUME as i64,
                0,
                0,
                0,
                0,
                4,
                4,
                3,
                gl::RGB as i64,
                gl::FLOAT as i64,
            ],
            Some(&bytes),
        )
        .unwrap();
        let pixels = sample(&mut context, VOLUME, "vec3(0.5,0.5,0.1666666667)", 0);
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 128, 255]),
            "initial float volume: {pixels:?}"
        );
        assert_eq!(
            call(
                &mut context,
                "enableExtension",
                &[],
                "EXT_color_buffer_float"
            ),
            json!(true)
        );
        super::array_copy_boundary_tests::framebuffer(&mut context, 0x8814);
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let charged = context.resource_bytes;
        upload(
            &mut context,
            "copyTexSubImage3D",
            &[VOLUME as i64, 0, 0, 0, 1, 0, 0, 4, 4],
            None,
        )
        .unwrap();
        assert_eq!(
            context.resource_bytes, charged,
            "scratch storage retires after conversion"
        );
        assert_eq!(
            context.objects.get(id, Kind::Texture).unwrap().native,
            unsafe {
                let mut binding = 0;
                gl::GetIntegerv(0x806a, &mut binding);
                binding as u32
            }
        );
        for layer in 0..3 {
            let pixels = sample(
                &mut context,
                VOLUME,
                &format!("vec3(0.5,0.5,{})", (layer as f64 + 0.5) / 3.0),
                0,
            );
            let expected = if layer == 1 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 128, 255]
            };
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel == expected),
                "layer {layer}: {pixels:?}"
            );
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
