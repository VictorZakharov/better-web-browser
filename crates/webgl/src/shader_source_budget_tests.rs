//! Large generated shader sources do not enlarge reflection/log allocations.
use super::api_version_tests::{call, compile, compiled, link, version_two};
use super::*;

fn source(bytes: usize, api: ApiVersion) -> String {
    let shader = if api == ApiVersion::Two {
        "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0,1,0,1);}\n"
    } else {
        "precision highp float;void main(){gl_FragColor=vec4(0,1,0,1);}\n"
    };
    format!("{shader}/*{}*/", "x".repeat(bytes - shader.len() - 4))
}

#[test]
fn large_sources_compile_and_render_in_both_webgl_versions_at_the_admitted_limit() {
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
            let vertex_source = if api == ApiVersion::Two {
                "#version 300 es\nvoid main(){vec2 p[3]=vec2[3](vec2(-1,-1),vec2(3,-1),vec2(-1,3));gl_Position=vec4(p[gl_VertexID],0,1);}"
            } else {
                "attribute vec2 position;void main(){gl_Position=vec4(position,0,1);}"
            };
            let vertex = compile(&mut context, gl::VERTEX_SHADER, vertex_source);
            let fragment = compile(
                &mut context,
                gl::FRAGMENT_SHADER,
                &source(MAX_SHADER_SOURCE_BYTES, api),
            );
            assert!(compiled(&mut context, vertex));
            assert!(compiled(&mut context, fragment));
            let program = link(&mut context, vertex, fragment);
            assert_eq!(
                call(
                    &mut context,
                    "getProgramParameter",
                    &[program as i64, gl::LINK_STATUS as i64],
                    ""
                ),
                json!(true)
            );
            let object = context.objects.get(fragment, Kind::Shader).unwrap();
            assert_eq!(object.bytes.len(), MAX_SHADER_SOURCE_BYTES);
            assert!(object.capacity >= MAX_SHADER_SOURCE_BYTES + MAX_SHADER_BYTES);
            if api == ApiVersion::Two {
                call(&mut context, "useProgram", &[program as i64], "");
                call(
                    &mut context,
                    "drawArrays",
                    &[gl::TRIANGLES as i64, 0, 3],
                    "",
                );
                assert!(
                    context
                        .surface
                        .snapshot()
                        .unwrap()
                        .chunks_exact(4)
                        .all(|p| p == [0, 255, 0, 255])
                );
            }
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn rejected_source_preserves_previous_source_compile_and_resource_accounting() {
    session::run_native_test(|| {
        let mut context = version_two();
        let original = source(80 * 1024, ApiVersion::Two);
        let id = compile(&mut context, gl::FRAGMENT_SHADER, &original);
        assert!(compiled(&mut context, id));
        let before = context.resource_bytes;
        for invalid in [
            source(MAX_SHADER_SOURCE_BYTES + 1, ApiVersion::Two),
            format!("{original}\0"),
        ] {
            let command = Command {
                op: "shaderSource".into(),
                i: vec![id as i64],
                f: vec![],
                text: invalid,
            };
            assert_eq!(context.dispatch(&command, None), Err(gl::INVALID_VALUE));
            assert_eq!(context.resource_bytes, before);
            assert_eq!(
                call(&mut context, "getShaderSource", &[id as i64], ""),
                json!(original)
            );
            assert!(compiled(&mut context, id));
        }
        let command = Command {
            op: "shaderSource".into(),
            i: vec![id as i64],
            f: vec![],
            text: source(MAX_SHADER_SOURCE_BYTES, ApiVersion::Two),
        };
        context.resource_limit = before;
        assert_eq!(context.dispatch(&command, None), Err(gl::OUT_OF_MEMORY));
        assert_eq!(
            call(&mut context, "getShaderSource", &[id as i64], ""),
            json!(original)
        );
        assert_eq!(context.resource_bytes, before);
    });
}

#[test]
fn serialized_source_admission_accounts_for_json_escape_expansion() {
    let mut contexts = Contexts::default();
    let id = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
    let shader = contexts
        .execute(id, r#"{"op":"createShader","i":[35632]}"#, None)
        .as_u64()
        .unwrap();
    let source = format!(
        "#version 300 es\n/*{}*/\nprecision highp float;out vec4 color;void main(){{color=vec4(1);}}",
        "\\\"\n".repeat(50_000)
    );
    assert!(source.len() < MAX_SHADER_SOURCE_BYTES);
    let command = json!({"op":"shaderSource","i":[shader],"text":source}).to_string();
    assert!(command.len() > MAX_SHADER_SOURCE_BYTES);
    assert!(command.len() < MAX_COMMAND_BYTES);
    contexts.execute(id, &command, None);
    assert_eq!(contexts.execute(id, r#"{"op":"getError"}"#, None), json!(0));
    let query = json!({"op":"getShaderSource","i":[shader]}).to_string();
    assert_eq!(contexts.execute(id, &query, None), json!(source));
    let compile = json!({"op":"compileShader","i":[shader]}).to_string();
    contexts.execute(id, &compile, None);
    let status = json!({"op":"getShaderParameter","i":[shader,gl::COMPILE_STATUS]}).to_string();
    assert_eq!(contexts.execute(id, &status, None), json!(true));
}
