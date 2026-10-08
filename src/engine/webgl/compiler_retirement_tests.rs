//! Ready native task payloads are retired without changing public compiler results.
use super::{
    api_version_tests::{call, compile, link, version_two},
    *,
};

const VERTEX: &str = "#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}";
const FRAGMENT: &str = "#version 300 es\nprecision highp float;uniform float shade;out vec4 color;void main(){color=vec4(shade,1,0,1);}";

#[test]
fn completed_shader_jobs_release_staging_without_erasing_source_or_status() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shader = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        assert_eq!(context.compiler_events.len(true), 1);
        assert_eq!(
            call(
                &mut context,
                "getShaderParameter",
                &[shader as i64, gl::COMPILE_STATUS as i64],
                ""
            ),
            json!(true)
        );
        context.retire_ready_compilers(4).unwrap();
        assert_eq!(context.compiler_events.len(true), 0);
        assert_eq!(
            call(&mut context, "getShaderSource", &[shader as i64], ""),
            json!(VERTEX)
        );
        assert_eq!(
            call(
                &mut context,
                "getShaderParameter",
                &[shader as i64, gl::COMPILE_STATUS as i64],
                ""
            ),
            json!(true)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn completed_link_jobs_preserve_locations_logs_and_reflection() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(&mut context, gl::FRAGMENT_SHADER, FRAGMENT);
        let program = link(&mut context, vertex, fragment);
        context.flush_program_links(1).unwrap();
        assert_eq!(context.compiler_events.len(false), 1);
        let location = call(
            &mut context,
            "getUniformLocation",
            &[program as i64],
            "shade",
        );
        context.retire_ready_compilers(4).unwrap();
        assert_eq!(context.compiler_events.len(false), 0);
        assert_eq!(
            call(
                &mut context,
                "getUniformLocation",
                &[program as i64],
                "shade"
            ),
            location
        );
        assert_eq!(
            call(&mut context, "getActiveUniform", &[program as i64, 0], "")["name"],
            json!("shade")
        );
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
            call(&mut context, "getProgramInfoLog", &[program as i64], ""),
            json!("")
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn failed_compilation_and_link_retirement_never_invents_success() {
    session::run_native_test(|| {
        let mut context = version_two();
        let bad = compile(&mut context, gl::VERTEX_SHADER, "not an ESSL shader");
        let fragment = compile(&mut context, gl::FRAGMENT_SHADER, FRAGMENT);
        let program = link(&mut context, bad, fragment);
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(false)
        );
        let log = call(&mut context, "getProgramInfoLog", &[program as i64], "");
        context.retire_ready_compilers(64).unwrap();
        assert_eq!(context.compiler_events.len(false), 0);
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
                "getShaderParameter",
                &[bad as i64, gl::COMPILE_STATUS as i64],
                ""
            ),
            json!(false)
        );
        assert_eq!(
            call(&mut context, "getProgramInfoLog", &[program as i64], ""),
            log
        );
        assert!(!log.as_str().unwrap().is_empty());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn retired_native_names_are_not_queried_after_browser_object_deletion() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shader = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        call(&mut context, "deleteShader", &[shader as i64], "");
        context.retire_ready_compilers(64).unwrap();
        assert_eq!(context.compiler_events.len(true), 0);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(&mut context, gl::FRAGMENT_SHADER, FRAGMENT);
        let program = link(&mut context, vertex, fragment);
        call(&mut context, "deleteProgram", &[program as i64], "");
        context.retire_ready_compilers(64).unwrap();
        assert_eq!(context.compiler_events.len(false), 0);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn many_successful_shader_submissions_apply_bounded_staging_backpressure() {
    session::run_native_test(|| {
        let mut context = version_two();
        let mut shaders = Vec::new();
        for _ in 0..compiler_events::SHADERS + 8 {
            shaders.push(compile(&mut context, gl::VERTEX_SHADER, VERTEX));
            assert!(context.compiler_events.len(true) <= compiler_events::SHADERS);
        }
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
        context
            .retire_ready_compilers(compiler_events::SHADERS)
            .unwrap();
        assert_eq!(context.compiler_events.len(true), 0);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn many_native_links_keep_staging_bounded_without_losing_executables() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(&mut context, gl::FRAGMENT_SHADER, FRAGMENT);
        let mut programs = Vec::new();
        for _ in 0..compiler_events::PROGRAMS * 3 {
            programs.push(link(&mut context, vertex, fragment));
            context.flush_program_links(1).unwrap();
            assert!(context.compiler_events.len(false) <= compiler_events::PROGRAMS);
        }
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
        }
        context
            .retire_ready_compilers(compiler_events::PROGRAMS)
            .unwrap();
        assert_eq!(context.compiler_events.len(false), 0);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
