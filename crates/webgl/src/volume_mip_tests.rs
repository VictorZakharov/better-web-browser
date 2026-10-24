//! Mip generation averages depth slices but never merges array layers.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::texture_targets::{ARRAY, VOLUME};
use super::*;

#[test]
fn webgl2_generated_volume_and_array_mips_keep_their_distinct_depth_semantics() {
    session::run_native_test(|| {
        let mut context = version_two();
        for target in [VOLUME, ARRAY] {
            let id = call(&mut context, "createTexture", &[], "")
                .as_u64()
                .unwrap() as u32;
            call(&mut context, "bindTexture", &[target as i64, id as i64], "");
            call(
                &mut context,
                "texParameteri",
                &[
                    target as i64,
                    gl::TEXTURE_MIN_FILTER as i64,
                    gl::NEAREST_MIPMAP_NEAREST as i64,
                ],
                "",
            );
            call(
                &mut context,
                "texParameteri",
                &[
                    target as i64,
                    gl::TEXTURE_MAG_FILTER as i64,
                    gl::NEAREST as i64,
                ],
                "",
            );
            let data: Vec<_> = (0..32)
                .flat_map(|pixel| {
                    if pixel < 16 {
                        [255, 0, 0, 255]
                    } else {
                        [0, 255, 0, 255]
                    }
                })
                .collect();
            let command = Command {
                op: "texImage3D".into(),
                i: vec![
                    target as i64,
                    0,
                    0x8058,
                    4,
                    4,
                    2,
                    0,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                ],
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, Some(&data)), Ok(Value::Null));
            call(&mut context, "generateMipmap", &[target as i64], "");
            let object = context.objects.get(id, Kind::Texture).unwrap();
            assert_eq!(
                object.core_images[&(target, 1)].depth,
                if target == VOLUME { 1 } else { 2 }
            );
            assert_eq!(
                object.core_images[&(target, 2)].depth,
                if target == VOLUME { 1 } else { 2 }
            );
            let sampler = if target == VOLUME {
                "sampler3D"
            } else {
                "sampler2DArray"
            };
            program(
                &mut context,
                VERTEX,
                &format!(
                    "#version 300 es\nprecision highp float;uniform highp {sampler} t;out vec4 color;void main(){{color=textureLod(t,vec3(0.5,0.5,0.0),1.0);}}"
                ),
            );
            call(
                &mut context,
                "drawArrays",
                &[gl::TRIANGLES as i64, 0, 3],
                "",
            );
            let pixels = context.surface.snapshot().unwrap();
            if target == ARRAY {
                assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
            } else {
                assert!(pixels.chunks_exact(4).all(|p| p[0].abs_diff(128) <= 1
                    && p[1].abs_diff(128) <= 1
                    && p[2] == 0
                    && p[3] == 255));
            }
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}
