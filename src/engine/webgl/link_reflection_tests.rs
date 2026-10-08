//! Deferred work must preserve the standard link-time state seen by reflection.
use super::{
    api_version_tests::{call, compile, link, version_two},
    *,
};

const FRAGMENT: &str = "#version 300 es\nprecision highp float;uniform float shade;out vec4 color;void main(){color=vec4(shade,1,0,1);}";
const VERTEX: &str = "#version 300 es\nin vec2 position;out vec2 first;out vec2 second;void main(){first=position;second=position*2.;gl_Position=vec4(position,0,1);}";

fn sources(context: &mut WebGl) -> [u32; 2] {
    [
        compile(context, gl::VERTEX_SHADER, VERTEX),
        compile(context, gl::FRAGMENT_SHADER, FRAGMENT),
    ]
}

#[test]
fn attribute_binding_after_link_applies_only_to_the_next_invocation() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shaders = sources(&mut context);
        let program = call(&mut context, "createProgram", &[], "")
            .as_u64()
            .unwrap() as u32;
        for shader in shaders {
            call(
                &mut context,
                "attachShader",
                &[program as i64, shader as i64],
                "",
            );
        }
        call(
            &mut context,
            "bindAttribLocation",
            &[program as i64, 2],
            "position",
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        assert!(context.pending_links.contains(program));
        call(
            &mut context,
            "bindAttribLocation",
            &[program as i64, 3],
            "position",
        );
        assert_eq!(
            call(
                &mut context,
                "getAttribLocation",
                &[program as i64],
                "position"
            ),
            json!(2)
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getAttribLocation",
                &[program as i64],
                "position"
            ),
            json!(3)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn transform_varying_update_keeps_the_previous_link_capture_list() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shaders = sources(&mut context);
        let program = call(&mut context, "createProgram", &[], "")
            .as_u64()
            .unwrap() as u32;
        for shader in shaders {
            call(
                &mut context,
                "attachShader",
                &[program as i64, shader as i64],
                "",
            );
        }
        call(
            &mut context,
            "transformFeedbackVaryings",
            &[program as i64, 0x8c8c],
            "[\"first\"]",
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        call(
            &mut context,
            "transformFeedbackVaryings",
            &[program as i64, 0x8c8c],
            "[\"second\"]",
        );
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[program as i64, 0],
                ""
            )["name"],
            json!("first")
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[program as i64, 0],
                ""
            )["name"],
            json!("second")
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn uniform_locations_expire_at_logical_relink_not_at_native_submission() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shaders = sources(&mut context);
        let program = link(&mut context, shaders[0], shaders[1]);
        let location = call(
            &mut context,
            "getUniformLocation",
            &[program as i64],
            "shade",
        )
        .as_u64()
        .unwrap() as u32;
        call(&mut context, "linkProgram", &[program as i64], "");
        assert!(context.pending_links.contains(program));
        assert!(context.objects.uniform_location(location).is_err());
        let new = call(
            &mut context,
            "getUniformLocation",
            &[program as i64],
            "shade",
        )
        .as_u64()
        .unwrap() as u32;
        assert_ne!(location, new);
        assert_eq!(
            call(&mut context, "getActiveUniform", &[program as i64, 0], "")["name"],
            json!("shade")
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn completion_poll_does_not_flush_unrelated_pending_invocations() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(
            &mut context,
            "enableExtension",
            &[],
            "KHR_parallel_shader_compile",
        );
        let shaders = sources(&mut context);
        let first = link(&mut context, shaders[0], shaders[1]);
        let second = link(&mut context, shaders[0], shaders[1]);
        // A poll may submit at most one ready native link. It cannot block to
        // resolve the entire queue merely because the requested object is later.
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[second as i64, 0x91b1],
                ""
            ),
            json!(false)
        );
        assert!(context.pending_links.contains(second));
        assert!(!context.pending_links.is_empty());
        for program in [first, second] {
            assert_eq!(
                call(
                    &mut context,
                    "getProgramParameter",
                    &[program as i64, gl::LINK_STATUS as i64],
                    ""
                ),
                json!(true)
            );
            assert_eq!(
                call(
                    &mut context,
                    "getProgramParameter",
                    &[program as i64, 0x91b1],
                    ""
                ),
                json!(true)
            );
        }
    });
}

#[test]
fn finished_failed_link_is_complete_but_not_successful() {
    session::run_native_test(|| {
        let mut context = version_two();
        call(
            &mut context,
            "enableExtension",
            &[],
            "KHR_parallel_shader_compile",
        );
        let shaders = sources(&mut context);
        let program = link(&mut context, shaders[0], shaders[1]);
        call(
            &mut context,
            "shaderSource",
            &[shaders[1] as i64],
            "not valid ESSL",
        );
        call(&mut context, "compileShader", &[shaders[1] as i64], "");
        call(&mut context, "linkProgram", &[program as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(false)
        );
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, 0x91b1],
                ""
            ),
            json!(true)
        );
        assert!(
            !call(&mut context, "getProgramInfoLog", &[program as i64], "")
                .as_str()
                .unwrap()
                .is_empty()
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn native_submission_backpressure_keeps_the_queue_bounded() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shaders = sources(&mut context);
        let mut programs = Vec::new();
        for _ in 0..pending_links::LIMIT + 4 {
            programs.push(link(&mut context, shaders[0], shaders[1]));
            assert!(context.pending_links.len() <= pending_links::LIMIT);
        }
        assert_eq!(context.pending_links.len(), pending_links::LIMIT);
        assert!(!context.pending_links.contains(programs[0]));
        for program in programs {
            assert_eq!(
                call(
                    &mut context,
                    "getProgramParameter",
                    &[program as i64, gl::LINK_STATUS as i64],
                    ""
                ),
                json!(true)
            );
            call(&mut context, "deleteProgram", &[program as i64], "");
        }
        assert!(context.pending_links.is_empty());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn task_boundary_progresses_real_submissions_without_author_polling() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shaders = sources(&mut context);
        let program = link(&mut context, shaders[0], shaders[1]);
        // Resolve only shader translation, not the queued program itself.
        for shader in shaders {
            assert_eq!(
                call(
                    &mut context,
                    "getShaderParameter",
                    &[shader as i64, gl::COMPILE_STATUS as i64],
                    ""
                ),
                json!(true)
            );
        }
        assert!(context.pending_links.contains(program));
        context.complete_gpu_task().unwrap();
        assert!(!context.pending_links.contains(program));
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(true)
        );
    });
}
