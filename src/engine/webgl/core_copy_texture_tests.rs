//! Real pixels distinguish sized framebuffer copies from enum-only admission.
use super::api_version_tests::{call, version_two};
use super::array_copy_boundary_tests::framebuffer;
use super::compressed_texture_tests::{texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

fn copy(context: &mut WebGl, internal: u32) -> Result<Value> {
    upload(
        context,
        "copyTexImage2D",
        &[gl::TEXTURE_2D as i64, 0, internal as i64, 0, 0, 4, 4, 0],
        None,
    )
}

fn sample_matches(context: &mut WebGl, sampler: &str, comparison: &str) {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;precision highp int;uniform highp {sampler} tex;out vec4 color;void main(){{color=({comparison})?vec4(0,1,0,1):vec4(1,0,0,1);}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    let pixels = context.surface.snapshot().unwrap();
    if !pixels
        .chunks_exact(4)
        .all(|pixel| pixel == [0, 255, 0, 255])
    {
        let components = &comparison[4..comparison.len() - 1];
        program(
            context,
            VERTEX,
            &format!(
                "#version 300 es\nprecision highp float;precision highp int;uniform highp {sampler} tex;out vec4 color;void main(){{color=vec4({components});}}"
            ),
        );
        call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
        panic!(
            "{sampler}: {comparison}; component matches={:?}",
            context.surface.snapshot().unwrap()
        );
    }
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255]),
        "{sampler}: {comparison}; pixels={pixels:?}"
    );
}

#[test]
fn webgl2_sized_normalized_copies_keep_native_texels_and_charge_storage() {
    session::run_native_test(|| {
        for internal in [0x8229, 0x822b, 0x8051, 0x8058] {
            let mut context = version_two();
            let source = framebuffer(&mut context, 0x8058);
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
            let destination = texture(&mut context, gl::TEXTURE_2D);
            let before = context.resource_bytes;
            copy(&mut context, internal).unwrap();
            assert_eq!(context.resource_bytes, before + 128);
            let image = context
                .objects
                .get(destination, Kind::Texture)
                .unwrap()
                .core_images[&(gl::TEXTURE_2D, 0)];
            assert_eq!(
                (image.internal, image.width, image.height),
                (internal, 4, 4)
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x8caa], ""),
                json!(source)
            );
            sample_matches(
                &mut context,
                "sampler2D",
                "all(equal(texelFetch(tex,ivec2(0),0),vec4(1,0,0,1)))",
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn webgl2_integer_copies_preserve_signed_and_unsigned_component_values() {
    session::run_native_test(|| {
        for (signed, source, destinations) in [
            (true, 0x8d82, [0x8235, 0x823b, 0x8d83, 0x8d82]),
            (false, 0x8d70, [0x8236, 0x823c, 0x8d71, 0x8d70]),
        ] {
            for (channels, internal) in destinations.into_iter().enumerate() {
                let mut context = version_two();
                framebuffer(&mut context, source);
                let values = if signed {
                    [-17, 7, i32::MIN as i64, i32::MAX as i64]
                } else {
                    [0xf1234567, 7, 0x80000000, 0xffffffff]
                };
                let mut arguments = vec![0x1800, 0];
                arguments.extend(values);
                call(
                    &mut context,
                    if signed {
                        "clearBufferiv"
                    } else {
                        "clearBufferuiv"
                    },
                    &arguments,
                    "",
                );
                texture(&mut context, gl::TEXTURE_2D);
                copy(&mut context, internal).unwrap();
                let expected = (0..4)
                    .map(|index| {
                        let value = if index <= channels {
                            values[index]
                        } else if index == 3 {
                            1
                        } else {
                            0
                        };
                        if signed {
                            format!("{value}")
                        } else {
                            format!("{value}u")
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(",");
                sample_matches(
                    &mut context,
                    if signed { "isampler2D" } else { "usampler2D" },
                    &format!(
                        "all(equal(texelFetch(tex,ivec2(0),0),{}({expected})))",
                        if signed { "ivec4" } else { "uvec4" }
                    ),
                );
                assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
            }
        }
    });
}

#[test]
fn webgl2_hdr_copies_retain_values_outside_normalized_range() {
    session::run_native_test(|| {
        for internal in [0x881a, 0x8814] {
            let mut context = version_two();
            assert_eq!(
                call(
                    &mut context,
                    "enableExtension",
                    &[],
                    "EXT_color_buffer_float"
                ),
                json!(true)
            );
            framebuffer(&mut context, internal);
            context
                .dispatch(
                    &Command {
                        op: "clearBufferfv".into(),
                        i: vec![0x1800, 0],
                        f: vec![4., -0.5, 0.25, 1.],
                        text: String::new(),
                    },
                    None,
                )
                .unwrap();
            texture(&mut context, gl::TEXTURE_2D);
            let before = context.resource_bytes;
            copy(&mut context, internal).unwrap();
            let bytes = core_texture_formats::storage(internal).unwrap().bytes;
            assert_eq!(context.resource_bytes, before + 32 * bytes);
            sample_matches(
                &mut context,
                "sampler2D",
                "all(lessThan(abs(texelFetch(tex,ivec2(0),0)-vec4(4,-0.5,0.25,1)),vec4(0.001)))",
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}
