//! Sized and immutable texture tests sample real initialized GPU storage.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

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

fn upload(context: &mut WebGl, op: &str, integers: &[i64], bytes: Option<&[u8]>) -> Result<Value> {
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

fn sample(context: &mut WebGl, body: &str, declaration: &str) -> Vec<u8> {
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;precision highp int;{declaration}\nout vec4 color;void main(){{{body}}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(0));
    context.surface.snapshot().unwrap()
}

#[test]
fn webgl2_immutable_levels_are_zero_initialized_and_cannot_be_redefined() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = texture(&mut context, gl::TEXTURE_2D);
        call(
            &mut context,
            "texStorage2D",
            &[gl::TEXTURE_2D as i64, 3, 0x8058, 4, 4],
            "",
        );
        call(
            &mut context,
            "texParameteri",
            &[
                gl::TEXTURE_2D as i64,
                gl::TEXTURE_MIN_FILTER as i64,
                gl::NEAREST_MIPMAP_NEAREST as i64,
            ],
            "",
        );
        assert_eq!(
            call(
                &mut context,
                "getTexParameter",
                &[gl::TEXTURE_2D as i64, 0x912f],
                ""
            ),
            json!(true)
        );
        assert_eq!(
            call(
                &mut context,
                "getTexParameter",
                &[gl::TEXTURE_2D as i64, 0x82df],
                ""
            ),
            json!(3)
        );
        assert_eq!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .len(),
            3
        );
        for level in 0..3 {
            let pixels = sample(
                &mut context,
                &format!("color=textureLod(t,vec2(0.5),float({level}));"),
                "uniform sampler2D t;",
            );
            assert!(pixels.iter().all(|byte| *byte == 0));
        }
        assert_eq!(
            upload(
                &mut context,
                "texImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    0x8058,
                    4,
                    4,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64
                ],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            upload(
                &mut context,
                "texStorage2D",
                &[gl::TEXTURE_2D as i64, 1, 0x8058, 1, 1],
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        let green = [0, 255, 0, 255];
        assert_eq!(
            upload(
                &mut context,
                "texSubImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    2,
                    0,
                    1,
                    1,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64
                ],
                Some(&green)
            ),
            Ok(Value::Null)
        );
        assert!(
            sample(
                &mut context,
                "color=textureLod(t,vec2(0.5),2.0);",
                "uniform sampler2D t;"
            )
            .chunks_exact(4)
            .all(|pixel| pixel == green)
        );
    });
}

#[test]
fn webgl2_sized_float_and_integer_textures_sample_actual_values() {
    session::run_native_test(|| {
        let mut context = version_two();
        texture(&mut context, gl::TEXTURE_2D);
        let float_bytes: Vec<_> = [2f32, 0., 0., 1.]
            .into_iter()
            .flat_map(f32::to_ne_bytes)
            .collect();
        assert_eq!(
            upload(
                &mut context,
                "texImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    0x881a,
                    1,
                    1,
                    0,
                    gl::RGBA as i64,
                    gl::FLOAT as i64
                ],
                Some(&float_bytes)
            ),
            Ok(Value::Null)
        );
        assert!(
            sample(
                &mut context,
                "color=vec4(texture(t,vec2(0.5)).r==2.0?1.0:0.0,0,0,1);",
                "uniform sampler2D t;"
            )
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255])
        );
        let integer_bytes = u32::MAX.to_ne_bytes();
        assert_eq!(
            upload(
                &mut context,
                "texImage2D",
                &[
                    gl::TEXTURE_2D as i64,
                    0,
                    0x8236,
                    1,
                    1,
                    0,
                    0x8d94,
                    gl::UNSIGNED_INT as i64
                ],
                Some(&integer_bytes)
            ),
            Ok(Value::Null)
        );
        assert!(
            sample(
                &mut context,
                "color=vec4(texture(t,vec2(0.5)).r==0xffffffffu?1.0:0.0,0,0,1);",
                "uniform highp usampler2D t;"
            )
            .chunks_exact(4)
            .all(|p| p == [255, 0, 0, 255])
        );
    });
}

#[test]
fn webgl2_storage_allocation_and_upload_failures_preserve_image_metadata() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = texture(&mut context, gl::TEXTURE_2D);
        for (args, error) in [
            (
                vec![gl::TEXTURE_2D as i64, 0, 0x8058, 4, 4],
                gl::INVALID_VALUE,
            ),
            (
                vec![gl::TEXTURE_2D as i64, 4, 0x8058, 4, 4],
                gl::INVALID_OPERATION,
            ),
            (
                vec![gl::TEXTURE_2D as i64, 1, gl::RGBA as i64, 4, 4],
                gl::INVALID_ENUM,
            ),
            (
                vec![gl::TEXTURE_2D as i64, 1, 0x8058, -1, 4],
                gl::INVALID_VALUE,
            ),
        ] {
            assert_eq!(
                upload(&mut context, "texStorage2D", &args, None),
                Err(error)
            );
        }
        assert!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .is_empty()
        );
        let red = [255, 0, 0, 255];
        let args = [
            gl::TEXTURE_2D as i64,
            0,
            0x8058,
            1,
            1,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
        ];
        assert_eq!(
            upload(&mut context, "texImage2D", &args, Some(&red[..3])),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            upload(&mut context, "texImage2D", &args, Some(&red)),
            Ok(Value::Null)
        );
        let bad = [
            gl::TEXTURE_2D as i64,
            0,
            0,
            1,
            1,
            0,
            gl::RGBA as i64,
            gl::FLOAT as i64,
        ];
        assert_eq!(
            upload(&mut context, "texSubImage2D", &bad, Some(&[0; 16])),
            Err(gl::INVALID_OPERATION)
        );
        assert!(
            sample(
                &mut context,
                "color=texture(t,vec2(0.5));",
                "uniform sampler2D t;"
            )
            .chunks_exact(4)
            .all(|p| p == red)
        );
    });
}
