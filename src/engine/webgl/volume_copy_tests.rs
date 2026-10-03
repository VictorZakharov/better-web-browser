//! Real GPU copies preserve neighboring slices and independent read/draw state.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{context, texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::texture_targets::{ARRAY, VOLUME};
use super::*;

fn clear(context: &mut WebGl, color: [f64; 4]) {
    context
        .dispatch(
            &Command {
                op: "clearColor".into(),
                i: vec![],
                f: color.to_vec(),
                text: String::new(),
            },
            None,
        )
        .unwrap();
    call(context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
}

fn storage(context: &mut WebGl, target: u32) -> u32 {
    let id = texture(context, target);
    call(
        context,
        "texStorage3D",
        &[target as i64, 2, 0x8058, 4, 4, 3],
        "",
    );
    id
}

fn sample(context: &mut WebGl, target: u32, uv: &str, level: i32) -> Vec<u8> {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    let sampler = if target == VOLUME {
        "sampler3D"
    } else {
        "sampler2DArray"
    };
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;uniform highp {sampler} t;out vec4 color;void main(){{color=textureLod(t,{uv},float({level}));}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    context.surface.snapshot().unwrap()
}

#[test]
fn framebuffer_copy_updates_only_the_selected_array_slice() {
    session::run_native_test(|| {
        let target = ARRAY;
        let mut context = version_two();
        storage(&mut context, target);
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let baseline = context.resource_bytes;
        call(
            &mut context,
            "copyTexSubImage3D",
            &[target as i64, 0, 0, 0, 1, 0, 0, 4, 4],
            "",
        );
        assert_eq!(
            context.resource_bytes, baseline,
            "copy does not allocate image storage"
        );
        for layer in 0..3 {
            let z = if target == VOLUME {
                (layer as f64 + 0.5) / 3.0
            } else {
                layer as f64
            };
            let pixels = sample(&mut context, target, &format!("vec3(0.5,0.5,{z})"), 0);
            let expected = if layer == 1 {
                [255, 0, 0, 255]
            } else {
                [0, 0, 0, 0]
            };
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel == expected),
                "target {target:#x}, layer {layer}: {pixels:?}"
            );
        }
    });
}

#[test]
fn pinned_provider_volume_copy_is_fail_closed_before_touching_native_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = storage(&mut context, VOLUME);
        let charged = context.resource_bytes;
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        assert_eq!(
            upload(
                &mut context,
                "copyTexSubImage3D",
                &[VOLUME as i64, 0, 0, 0, 1, 0, 0, 4, 4],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(context.resource_bytes, charged);
        let pixels = sample(&mut context, VOLUME, "vec3(0.5,0.5,0.5)", 0);
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 0]));
        assert_eq!(
            context.objects.get(id, Kind::Texture).unwrap().core_images[&(VOLUME, 0)].depth,
            3
        );
    });
}

#[test]
fn subregion_copy_preserves_unmodified_texels_and_immutable_mip_extents() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = storage(&mut context, ARRAY);
        clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        call(
            &mut context,
            "copyTexSubImage3D",
            &[ARRAY as i64, 0, 2, 0, 2, 0, 0, 2, 4],
            "",
        );
        for (uv, expected) in [
            ("vec3(0.25,0.5,2)", [0, 0, 0, 0]),
            ("vec3(0.75,0.5,2)", [0, 255, 0, 255]),
            ("vec3(0.75,0.5,1)", [0, 0, 0, 0]),
        ] {
            let pixels = sample(&mut context, ARRAY, uv, 0);
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
        clear(&mut context, [0.0, 0.0, 1.0, 1.0]);
        call(
            &mut context,
            "copyTexSubImage3D",
            &[ARRAY as i64, 1, 0, 0, 2, 0, 0, 2, 2],
            "",
        );
        call(
            &mut context,
            "texParameteri",
            &[
                ARRAY as i64,
                gl::TEXTURE_MIN_FILTER as i64,
                gl::NEAREST_MIPMAP_NEAREST as i64,
            ],
            "",
        );
        let pixels = sample(&mut context, ARRAY, "vec3(0.5,0.5,2)", 1);
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 255, 255])
        );
        let object = context.objects.get(id, Kind::Texture).unwrap();
        assert_eq!(object.immutable_levels, 2);
        assert_eq!(object.core_images[&(ARRAY, 1)].width, 2);
        assert_eq!(object.core_images[&(ARRAY, 1)].depth, 3);
    });
}

#[test]
fn volume_copy_rejects_bad_regions_without_changing_destination_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = storage(&mut context, ARRAY);
        let args = [ARRAY as i64, 0, 0, 0, 1, 0, 0, 4, 4];
        let baseline = context.resource_bytes;
        for (index, value, error) in [
            (0, gl::TEXTURE_2D as i64, gl::INVALID_ENUM),
            (1, -1, gl::INVALID_VALUE),
            (1, 13, gl::INVALID_VALUE),
            (1, 2, gl::INVALID_OPERATION),
            (2, -1, gl::INVALID_VALUE),
            (2, 1, gl::INVALID_VALUE),
            (3, -1, gl::INVALID_VALUE),
            (3, 1, gl::INVALID_VALUE),
            (4, -1, gl::INVALID_VALUE),
            (4, 3, gl::INVALID_VALUE),
            (7, -1, gl::INVALID_VALUE),
            (7, i32::MAX as i64, gl::INVALID_VALUE),
            (8, -1, gl::INVALID_VALUE),
            (8, i32::MAX as i64, gl::INVALID_VALUE),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(&mut context, "copyTexSubImage3D", &invalid, None),
                Err(error),
                "argument {index}={value}"
            );
            assert_eq!(context.resource_bytes, baseline);
        }
        let pixels = sample(&mut context, ARRAY, "vec3(0.5,0.5,1)", 0);
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 0]));
        assert_eq!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .len(),
            2
        );
    });
}

#[test]
fn gpu_volume_copy_does_not_interpret_pixel_unpack_buffer_or_store_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        storage(&mut context, ARRAY);
        clear(&mut context, [1.0, 0.0, 1.0, 1.0]);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindBuffer", &[0x88ec, buffer as i64], "");
        call(
            &mut context,
            "bufferData",
            &[0x88ec, 1, gl::STATIC_DRAW as i64],
            "",
        );
        for (parameter, value) in [(0x0cf2, 32), (0x0cf3, 10), (0x806e, 32), (0x806d, 7)] {
            call(&mut context, "pixelStorei", &[parameter, value], "");
        }
        call(
            &mut context,
            "copyTexSubImage3D",
            &[ARRAY as i64, 0, 0, 0, 1, 0, 0, 4, 4],
            "",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x88ef], ""),
            json!(buffer)
        );
        for (parameter, value) in [(0x0cf2, 32), (0x0cf3, 10), (0x806e, 32), (0x806d, 7)] {
            assert_eq!(
                call(&mut context, "getParameter", &[parameter], ""),
                json!(value)
            );
        }
        let pixels = sample(&mut context, ARRAY, "vec3(0.5,0.5,1)", 0);
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 255, 255])
        );
    });
}

#[test]
fn staged_volume_copy_never_upgrades_a_public_webgl1_context() {
    session::run_native_test(|| {
        let mut context = context();
        assert_eq!(
            upload(
                &mut context,
                "copyTexSubImage3D",
                &[ARRAY as i64, 0, 0, 0, 0, 0, 0, 4, 4],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}
