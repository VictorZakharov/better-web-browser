//! The last legal gl_VertexID is inclusive; one vertex at INT_MAX is valid.
use super::api_version_tests::{call, version_two};
use super::array_copy_boundary_tests::framebuffer;
use super::compressed_texture_tests::upload;
use super::core_uniform_tests::program;
use super::*;

#[test]
fn vertex_id_at_the_signed_limit_renders_real_integer_pixels() {
    session::run_native_test(|| {
        for instanced in [false, true] {
            let mut context = version_two();
            framebuffer(&mut context, 0x8d82);
            program(
                &mut context,
                "#version 300 es\nflat out int id;void main(){id=gl_VertexID;gl_Position=vec4(0,0,0,1);gl_PointSize=4.;}",
                "#version 300 es\nprecision highp float;flat in int id;out ivec4 color;void main(){color=ivec4(id,0,0,1);}",
            );
            for first in [0i64, 1_000_000, i32::MAX as i64 - 1, i32::MAX as i64] {
                let mut args = vec![gl::POINTS as i64, first, 1];
                if instanced {
                    args.push(1);
                }
                let result = upload(
                    &mut context,
                    if instanced {
                        "drawArraysInstanced"
                    } else {
                        "drawArrays"
                    },
                    &args,
                    None,
                );
                // Native backends may refuse the enormous base index with OOM,
                // but not INVALID_VALUE for a representable last vertex.
                if result == Err(gl::OUT_OF_MEMORY) {
                    continue;
                }
                result.unwrap();
                let pixels = context
                    .read_pixels(
                        &Command {
                            op: "readPixels".into(),
                            i: vec![
                                0,
                                0,
                                4,
                                4,
                                core_texture_formats::RGBA_INTEGER as i64,
                                gl::INT as i64,
                                256,
                            ],
                            f: vec![],
                            text: String::new(),
                        },
                        None,
                    )
                    .unwrap();
                assert!(pixels.chunks_exact(16).all(|pixel| i32::from_ne_bytes(
                    pixel[..4].try_into().unwrap()
                ) == first as i32));
            }
            for (first, count) in [
                (i32::MAX as i64 + 1, 0),
                (i32::MAX as i64 + 1, 1),
                (i32::MAX as i64, 2),
                (-1, 1),
            ] {
                let mut args = vec![gl::POINTS as i64, first, count];
                if instanced {
                    args.push(1);
                }
                assert_eq!(
                    upload(
                        &mut context,
                        if instanced {
                            "drawArraysInstanced"
                        } else {
                            "drawArrays"
                        },
                        &args,
                        None
                    ),
                    Err(gl::INVALID_VALUE)
                );
            }
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}
