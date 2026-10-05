use super::*;
use crate::engine::webgl::api_version_tests::{call, compile, compiled, link, version_two};
use crate::engine::webgl::*;

fn command(op: &str, values: &[i64]) -> Command {
    Command {
        op: op.into(),
        i: values.into(),
        f: vec![],
        text: String::new(),
    }
}

fn program(context: &mut WebGl) {
    assert_eq!(
        call(context, "enableExtension", &[], "WEBGL_multi_draw"),
        json!(true)
    );
    let vertex = compile(
        context,
        gl::VERTEX_SHADER,
        "#version 300 es\n#extension GL_ANGLE_multi_draw : require\nflat out int draw;void main(){draw=gl_DrawID;gl_Position=vec4(float(gl_DrawID)*0.5-0.75,0.25,0,1);gl_PointSize=1.0;}",
    );
    assert!(
        compiled(context, vertex),
        "{:?}",
        call(context, "getShaderInfoLog", &[vertex as i64], "")
    );
    let fragment = compile(
        context,
        gl::FRAGMENT_SHADER,
        "#version 300 es\nprecision highp float;flat in int draw;out vec4 color;void main(){color=draw==0?vec4(1,0,0,1):draw==1?vec4(0,1,0,1):vec4(0,0,1,1);}",
    );
    assert!(compiled(context, fragment));
    let linked = link(context, vertex, fragment);
    assert_eq!(
        call(
            context,
            "getProgramParameter",
            &[linked as i64, gl::LINK_STATUS as i64],
            ""
        ),
        json!(true)
    );
    call(context, "useProgram", &[linked as i64], "");
}

fn pixel(context: &WebGl, x: usize) -> Vec<u8> {
    // Surface snapshots use top-left rows, unlike WebGL readPixels.
    context.surface.snapshot().unwrap()[(4 + x) * 4..(4 + x) * 4 + 4].to_vec()
}

#[test]
fn multi_draw_id_preserves_empty_slots_and_ordinary_draws_reset_to_zero() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context);
        call(
            &mut context,
            "multiDrawArraysWEBGL",
            &[gl::POINTS as i64, 0, 3, 0, 0, 1, 0, 1, 1, 0, 1, 1],
            "",
        );
        assert_eq!(pixel(&context, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&context, 1), [0, 255, 0, 255]);
        assert_eq!(pixel(&context, 2), [0, 0, 255, 255]);
        call(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], "");
        assert_eq!(pixel(&context, 0), [255, 0, 0, 255]);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn multi_draw_late_invalid_ranges_do_not_partially_render() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context);
        for values in [
            vec![gl::POINTS as i64, 0, 2, 0, 1, 1, -1, 1, 1],
            vec![gl::POINTS as i64, 0, 2, 0, 1, 1, i32::MAX as i64, 2, 1],
        ] {
            assert_eq!(
                context.dispatch(&command("multiDrawArraysWEBGL", &values), None),
                Err(gl::INVALID_VALUE)
            );
            assert!(context.surface.snapshot().unwrap().iter().all(|v| *v == 0));
        }
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindBuffer",
            &[gl::ELEMENT_ARRAY_BUFFER as i64, buffer],
            "",
        );
        context
            .dispatch(
                &command(
                    "bufferData",
                    &[gl::ELEMENT_ARRAY_BUFFER as i64, 1, gl::STATIC_DRAW as i64],
                ),
                Some(&[0]),
            )
            .unwrap();
        assert_eq!(
            context.dispatch(
                &command(
                    "multiDrawElementsWEBGL",
                    &[
                        gl::POINTS as i64,
                        gl::UNSIGNED_BYTE as i64,
                        2,
                        0,
                        1,
                        1,
                        1,
                        1,
                        1
                    ]
                ),
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert!(context.surface.snapshot().unwrap().iter().all(|v| *v == 0));
        call(
            &mut context,
            "multiDrawElementsWEBGL",
            &[
                gl::POINTS as i64,
                gl::UNSIGNED_BYTE as i64,
                2,
                0,
                1,
                1,
                0,
                1,
                1,
            ],
            "",
        );
        assert_eq!(pixel(&context, 0), [255, 0, 0, 255]);
        assert_eq!(pixel(&context, 1), [0, 255, 0, 255]);
    });
}

#[test]
fn multi_draw_requires_admission_and_bounds_owned_command_shapes() {
    session::run_native_test(|| {
        let mut context = version_two();
        let zero = command("multiDrawArraysWEBGL", &[gl::POINTS as i64, 0, 0]);
        assert_eq!(context.dispatch(&zero, None), Err(gl::INVALID_OPERATION));
        program(&mut context);
        assert_eq!(context.dispatch(&zero, None), Ok(Value::Null));
        for (values, error) in [
            (vec![0xffff, 0, 0], gl::INVALID_ENUM),
            (vec![gl::POINTS as i64, 0, -1], gl::INVALID_VALUE),
            (
                vec![gl::POINTS as i64, 0, MAX_DRAWS as i64 + 1],
                gl::OUT_OF_MEMORY,
            ),
            (vec![gl::POINTS as i64, 0, 1], gl::INVALID_VALUE),
            (vec![gl::POINTS as i64, 0, 1, 0, 1, 2], gl::INVALID_VALUE),
            (
                vec![gl::POINTS as i64, 0, 1, 0, MAX_DRAW_VERTICES as i64 + 1, 1],
                gl::INVALID_VALUE,
            ),
        ] {
            assert_eq!(
                context.dispatch(&command("multiDrawArraysWEBGL", &values), None),
                Err(error)
            );
        }
        assert!(context.surface.snapshot().unwrap().iter().all(|v| *v == 0));
    });
}

#[test]
fn multi_draw_instanced_variants_use_native_draw_id_without_compacting_zero_instances() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context);
        call(
            &mut context,
            "multiDrawArraysInstancedWEBGL",
            &[gl::POINTS as i64, 0, 3, 0, 1, 0, 0, 1, 2, 0, 1, 1],
            "",
        );
        assert_eq!(pixel(&context, 0), [0, 0, 0, 0]);
        assert_eq!(pixel(&context, 1), [0, 255, 0, 255]);
        assert_eq!(pixel(&context, 2), [0, 0, 255, 255]);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindBuffer",
            &[gl::ELEMENT_ARRAY_BUFFER as i64, buffer],
            "",
        );
        context
            .dispatch(
                &command(
                    "bufferData",
                    &[gl::ELEMENT_ARRAY_BUFFER as i64, 1, gl::STATIC_DRAW as i64],
                ),
                Some(&[0]),
            )
            .unwrap();
        call(
            &mut context,
            "multiDrawElementsInstancedWEBGL",
            &[gl::POINTS as i64, gl::UNSIGNED_BYTE as i64, 1, 0, 1, 1],
            "",
        );
        assert_eq!(pixel(&context, 0), [255, 0, 0, 255]);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
