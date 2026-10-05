//! Genuine reserved-name queries must not poison framework rendering errors.
use super::api_version_tests::{call, compile, compiled, link};
use super::*;

fn query(context: &mut WebGl, op: &str, program: u32, name: &str) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: vec![program as i64, 0],
            f: vec![],
            text: name.into(),
        },
        None,
    )
}

fn program(context: &mut WebGl, api: ApiVersion) -> u32 {
    let vertex = if api == ApiVersion::Two {
        "#version 300 es\nin vec2 position;void main(){gl_Position=vec4(position+float(gl_VertexID)*.01,0,1);}"
    } else {
        "attribute vec2 position;void main(){gl_Position=vec4(position,0,1);}"
    };
    let fragment = if api == ApiVersion::Two {
        "#version 300 es\nprecision highp float;uniform vec4 tint;out vec4 color;void main(){color=tint;}"
    } else {
        "precision highp float;uniform vec4 tint;void main(){gl_FragColor=tint;}"
    };
    let vertex = compile(context, gl::VERTEX_SHADER, vertex);
    let fragment = compile(context, gl::FRAGMENT_SHADER, fragment);
    assert!(compiled(context, vertex) && compiled(context, fragment));
    let program = link(context, vertex, fragment);
    assert_eq!(
        call(
            context,
            "getProgramParameter",
            &[program as i64, gl::LINK_STATUS as i64],
            ""
        ),
        json!(true)
    );
    program
}

#[test]
fn reserved_location_queries_are_absent_without_error_but_binding_is_invalid() {
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
            let program = program(&mut context, api);
            let location = query(&mut context, "getAttribLocation", program, "position").unwrap();
            assert!(location.as_i64().unwrap() >= 0);
            for name in [
                "gl_VertexID",
                "gl_InstanceID",
                "gl_Position",
                "gl_unknown",
                "webgl_private",
                "_webgl_private",
            ] {
                assert_eq!(
                    query(&mut context, "getAttribLocation", program, name),
                    Ok(json!(-1))
                );
                assert_eq!(
                    query(&mut context, "getUniformLocation", program, name),
                    Ok(Value::Null)
                );
                assert_eq!(
                    query(&mut context, "bindAttribLocation", program, name),
                    Err(gl::INVALID_OPERATION)
                );
                assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
                assert_eq!(
                    query(&mut context, "getAttribLocation", program, "position").unwrap(),
                    location
                );
            }
            assert!(
                query(&mut context, "getUniformLocation", program, "tint")
                    .unwrap()
                    .as_u64()
                    .is_some()
            );
            let unlinked = call(&mut context, "createProgram", &[], "")
                .as_u64()
                .unwrap() as u32;
            for op in ["getAttribLocation", "getUniformLocation"] {
                assert_eq!(
                    query(&mut context, op, unlinked, "gl_Position"),
                    Err(gl::INVALID_OPERATION)
                );
                assert_eq!(
                    query(&mut context, op, unlinked, "position"),
                    Err(gl::INVALID_OPERATION)
                );
            }
        }
    });
}

#[test]
fn query_name_character_set_distinguishes_glsl_characters_from_ascii_and_unicode() {
    let excluded = [b'"', b'\'', b'$', b'@', b'\\', b'`'];
    for byte in 0..=255u8 {
        let value = char::from(byte).to_string();
        let valid =
            (9..=13).contains(&byte) || (32..=126).contains(&byte) && !excluded.contains(&byte);
        assert_eq!(
            shader_names::validate(&value, 1024).is_ok(),
            valid,
            "byte {byte}"
        );
    }
    assert!(shader_names::location(&"a".repeat(256), ApiVersion::One).is_ok());
    assert_eq!(
        shader_names::location(&"a".repeat(257), ApiVersion::One),
        Err(gl::INVALID_VALUE)
    );
    assert!(shader_names::location(&"a".repeat(1024), ApiVersion::Two).is_ok());
    assert_eq!(
        shader_names::location(&"a".repeat(1025), ApiVersion::Two),
        Err(gl::INVALID_VALUE)
    );
}

#[test]
fn malformed_query_strings_fail_without_rebinding_or_changing_valid_locations() {
    session::run_native_test(|| {
        let mut context = api_version_tests::version_two();
        let program = program(&mut context, ApiVersion::Two);
        let before = query(&mut context, "getUniformLocation", program, "tint").unwrap();
        for name in [
            "tint\0hidden",
            "tint\u{7}",
            "tint@",
            "tint$",
            "tint\\",
            "tint`",
            "tint\"",
            "tint'",
            "é",
        ] {
            for op in [
                "getAttribLocation",
                "getUniformLocation",
                "bindAttribLocation",
            ] {
                assert_eq!(
                    query(&mut context, op, program, name),
                    Err(gl::INVALID_VALUE)
                );
            }
            for op in ["getFragDataLocation", "getUniformBlockIndex"] {
                assert_eq!(
                    query(&mut context, op, program, name),
                    Err(gl::INVALID_VALUE)
                );
            }
            let names = json!(["tint", name]).to_string();
            assert_eq!(
                query(&mut context, "getUniformIndices", program, &names),
                Err(gl::INVALID_VALUE)
            );
            assert_eq!(
                query(&mut context, "getUniformLocation", program, "tint").unwrap(),
                before
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}
