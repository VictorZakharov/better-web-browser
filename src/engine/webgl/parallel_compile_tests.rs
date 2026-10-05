use super::api_version_tests::{call, compile, compiled, link};
use super::*;

const COMPLETION: i64 = 0x91b1;

fn query(context: &mut WebGl, shader: bool, id: u32) -> Result<Value> {
    context.dispatch(
        &Command {
            op: if shader {
                "getShaderParameter"
            } else {
                "getProgramParameter"
            }
            .into(),
            i: vec![id as i64, COMPLETION],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

#[test]
fn parallel_completion_requires_native_availability_and_author_admission() {
    session::run_native_test(|| {
        for api in [ApiVersion::One, ApiVersion::Two] {
            let mut context = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Options::default()
                },
            )
            .unwrap();
            let program = call(&mut context, "createProgram", &[], "")
                .as_u64()
                .unwrap() as u32;
            let shader = call(
                &mut context,
                "createShader",
                &[gl::VERTEX_SHADER as i64],
                "",
            )
            .as_u64()
            .unwrap() as u32;
            assert_eq!(query(&mut context, false, program), Err(gl::INVALID_ENUM));
            assert_eq!(query(&mut context, true, shader), Err(gl::INVALID_ENUM));
            let available = context.extensions.available_parallel_compile;
            let names = call(&mut context, "supportedExtensions", &[], "");
            assert_eq!(
                names
                    .as_array()
                    .unwrap()
                    .contains(&json!("KHR_parallel_shader_compile")),
                available
            );
            assert_eq!(
                call(
                    &mut context,
                    "enableExtension",
                    &[],
                    "KHR_parallel_shader_compile"
                ),
                json!(available)
            );
            if available {
                assert_eq!(query(&mut context, false, program), Ok(json!(true)));
                assert_eq!(query(&mut context, true, shader), Ok(json!(true)));
                assert_eq!(
                    query(&mut context, false, shader),
                    Err(gl::INVALID_OPERATION)
                );
                assert_eq!(
                    query(&mut context, true, program),
                    Err(gl::INVALID_OPERATION)
                );
            } else {
                assert_eq!(query(&mut context, false, program), Err(gl::INVALID_ENUM));
            }
            assert_eq!(call(&mut context, "getError", &[], ""), json!(gl::NO_ERROR));
        }
    });
}

#[test]
fn native_completion_reports_finished_failed_and_successful_compiles_without_faking_link_status() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        let available = context.extensions.available_parallel_compile;
        assert_eq!(
            call(
                &mut context,
                "enableExtension",
                &[],
                "KHR_parallel_shader_compile"
            ),
            json!(available)
        );
        if !available {
            return;
        }
        let vertex = compile(
            &mut context,
            gl::VERTEX_SHADER,
            "void main(){gl_Position=vec4(0.0);}",
        );
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "precision mediump float; void main(){gl_FragColor=vec4(1.0);}",
        );
        let program = link(&mut context, vertex, fragment);
        // A background compiler may already finish this small input. Do not
        // require an artificial pending interval or conflate completion with success.
        assert!(query(&mut context, false, program).unwrap().is_boolean());
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(true)
        );
        assert_eq!(query(&mut context, false, program), Ok(json!(true)));
        assert_eq!(query(&mut context, true, vertex), Ok(json!(true)));
        let invalid = compile(&mut context, gl::VERTEX_SHADER, "this is not a shader");
        assert!(!compiled(&mut context, invalid));
        assert_eq!(query(&mut context, true, invalid), Ok(json!(true)));
        let failed = link(&mut context, invalid, fragment);
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[failed as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(false)
        );
        assert_eq!(query(&mut context, false, failed), Ok(json!(true)));
        assert_eq!(call(&mut context, "getError", &[], ""), json!(gl::NO_ERROR));
    });
}
