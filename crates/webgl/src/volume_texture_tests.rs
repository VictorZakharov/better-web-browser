//! GPU sampling distinguishes volume slices from array layers and checks bounds.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::texture_targets::{ARRAY, VOLUME};
use super::*;

fn command(context: &mut WebGl, op: &str, args: &[i64], bytes: Option<&[u8]>) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: args.into(),
            f: vec![],
            text: String::new(),
        },
        bytes,
    )
}
fn texture(context: &mut WebGl, target: u32) -> u32 {
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
fn sample(context: &mut WebGl, target: u32, layer: &str) -> Vec<u8> {
    let sampler = if target == VOLUME {
        "sampler3D"
    } else {
        "sampler2DArray"
    };
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;uniform highp {sampler} t;out vec4 color;void main(){{color=texture(t,vec3(0.5,0.5,{layer}));}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(0));
    context.surface.snapshot().unwrap()
}

#[test]
fn webgl2_volume_and_array_uploads_sample_distinct_real_slices() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [VOLUME, ARRAY] {
            texture(&mut context, target);
            let bytes: Vec<_> = (0..8)
                .flat_map(|pixel| {
                    if pixel < 4 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 255]
                    }
                })
                .collect();
            assert_eq!(
                command(
                    &mut context,
                    "texImage3D",
                    &[
                        target as i64,
                        0,
                        0x8058,
                        2,
                        2,
                        2,
                        0,
                        gl::RGBA as i64,
                        gl::UNSIGNED_BYTE as i64
                    ],
                    Some(&bytes)
                ),
                Ok(Value::Null)
            );
            let first = if target == VOLUME { "0.25" } else { "0.0" };
            let second = if target == VOLUME { "0.75" } else { "1.0" };
            assert!(
                sample(&mut context, target, first)
                    .chunks_exact(4)
                    .all(|p| p == [255, 0, 0, 255])
            );
            assert!(
                sample(&mut context, target, second)
                    .chunks_exact(4)
                    .all(|p| p == [0, 255, 0, 255])
            );
            let blue: Vec<_> = (0..4).flat_map(|_| [0, 0, 255, 255]).collect();
            assert_eq!(
                command(
                    &mut context,
                    "texSubImage3D",
                    &[
                        target as i64,
                        0,
                        0,
                        0,
                        0,
                        2,
                        2,
                        1,
                        gl::RGBA as i64,
                        gl::UNSIGNED_BYTE as i64
                    ],
                    Some(&blue)
                ),
                Ok(Value::Null)
            );
            assert!(
                sample(&mut context, target, first)
                    .chunks_exact(4)
                    .all(|p| p == [0, 0, 255, 255])
            );
            assert!(
                sample(&mut context, target, second)
                    .chunks_exact(4)
                    .all(|p| p == [0, 255, 0, 255])
            );
        }
    });
}

#[test]
fn webgl2_volume_mips_shrink_depth_but_array_mips_keep_every_layer() {
    session::run_native_test(|| {
        let mut context = version_two();
        for (target, levels) in [(VOLUME, 4), (ARRAY, 2)] {
            let id = texture(&mut context, target);
            call(
                &mut context,
                "texStorage3D",
                &[target as i64, levels, 0x8058, 2, 2, 8],
                "",
            );
            let object = context.objects.get(id, Kind::Texture).unwrap();
            assert_eq!(object.core_images.len(), levels as usize);
            for level in 0..levels as i32 {
                let image = object.core_images[&(target, level)];
                assert_eq!(image.width, (2 >> level).max(1));
                assert_eq!(
                    image.depth,
                    if target == VOLUME {
                        (8 >> level).max(1)
                    } else {
                        8
                    }
                );
            }
            assert!(
                sample(&mut context, target, "0.0")
                    .iter()
                    .all(|byte| *byte == 0)
            );
        }
        let id = texture(&mut context, ARRAY);
        assert_eq!(
            command(
                &mut context,
                "texStorage3D",
                &[ARRAY as i64, 4, 0x8058, 2, 2, 8],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .is_empty()
        );
    });
}

#[test]
fn webgl2_volume_upload_bounds_alignment_and_immutable_failures_are_atomic() {
    session::run_native_test(|| {
        let mut context = version_two();
        texture(&mut context, VOLUME);
        call(
            &mut context,
            "texStorage3D",
            &[VOLUME as i64, 1, 0x8058, 2, 2, 2],
            "",
        );
        let bytes = [255; 32];
        for (args, size, error) in [
            (
                vec![
                    VOLUME as i64,
                    0,
                    0,
                    0,
                    0,
                    2,
                    2,
                    2,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                ],
                31,
                gl::INVALID_OPERATION,
            ),
            (
                vec![
                    VOLUME as i64,
                    0,
                    0,
                    0,
                    1,
                    2,
                    2,
                    2,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                ],
                32,
                gl::INVALID_VALUE,
            ),
            (
                vec![
                    VOLUME as i64,
                    0,
                    0,
                    0,
                    -1,
                    2,
                    2,
                    1,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                ],
                16,
                gl::INVALID_VALUE,
            ),
        ] {
            assert_eq!(
                command(&mut context, "texSubImage3D", &args, Some(&bytes[..size])),
                Err(error)
            );
        }
        assert_eq!(
            command(
                &mut context,
                "texImage3D",
                &[
                    VOLUME as i64,
                    0,
                    0x8058,
                    2,
                    2,
                    2,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64
                ],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert!(
            sample(&mut context, VOLUME, "0.75")
                .iter()
                .all(|byte| *byte == 0)
        );
        texture(&mut context, ARRAY);
        let before = context.resource_bytes;
        assert_eq!(
            command(
                &mut context,
                "texStorage3D",
                &[ARRAY as i64, 1, 0x8058, 1024, 1024, 256],
                None
            ),
            Err(gl::OUT_OF_MEMORY)
        );
        assert_eq!(context.resource_bytes, before);
    });
}

#[test]
fn webgl2_volume_bindings_have_independent_targets_and_deleted_public_names() {
    session::run_native_test(|| {
        let mut context = version_two();
        let volume = texture(&mut context, VOLUME);
        let array = texture(&mut context, ARRAY);
        assert_eq!(
            call(&mut context, "getParameter", &[0x806a], ""),
            json!(volume)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8c1d], ""),
            json!(array)
        );
        assert_eq!(
            command(
                &mut context,
                "bindTexture",
                &[ARRAY as i64, volume as i64],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "deleteTexture", &[volume as i64], "");
        assert_eq!(
            call(&mut context, "getParameter", &[0x806a], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8c1d], ""),
            json!(array)
        );
        let mut one = WebGl::new(2, 2, Options::default()).unwrap();
        assert_eq!(
            command(&mut one, "bindTexture", &[VOLUME as i64, 0], None),
            Err(gl::INVALID_ENUM)
        );
        assert_eq!(
            command(
                &mut one,
                "texStorage3D",
                &[VOLUME as i64, 1, 0x8058, 2, 2, 2],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}
