//! Native WebGL2 milestones do not expose a partial canvas API to sites.
use super::*;

pub(super) fn version_two() -> WebGl {
    WebGl::new(
        4,
        4,
        Options {
            api: ApiVersion::Two,
            ..Options::default()
        },
    )
    .expect("real WebGL-compatible GLES3 context")
}

pub(super) fn call(context: &mut WebGl, op: &str, integers: &[i64], source: &str) -> Value {
    let command = Command {
        op: op.into(),
        i: integers.into(),
        f: vec![],
        text: source.into(),
    };
    context
        .dispatch(&command, None)
        .unwrap_or_else(|error| panic!("{op}: {error:#x}"))
}

pub(super) fn compile(context: &mut WebGl, kind: u32, source: &str) -> u32 {
    let id = call(context, "createShader", &[kind as i64], "")
        .as_u64()
        .unwrap() as u32;
    call(context, "shaderSource", &[id as i64], source);
    call(context, "compileShader", &[id as i64], "");
    id
}

pub(super) fn compiled(context: &mut WebGl, id: u32) -> bool {
    call(
        context,
        "getShaderParameter",
        &[id as i64, gl::COMPILE_STATUS as i64],
        "",
    ) == json!(true)
}

pub(super) fn link(context: &mut WebGl, vertex: u32, fragment: u32) -> u32 {
    let id = call(context, "createProgram", &[], "").as_u64().unwrap() as u32;
    for shader in [vertex, fragment] {
        call(context, "attachShader", &[id as i64, shader as i64], "");
    }
    call(context, "linkProgram", &[id as i64], "");
    id
}

#[test]
fn webgl2_native_version_and_robust_zero_initialization_are_real() {
    session::run_native_test(|| {
        let mut context = version_two();
        let version = unsafe { std::ffi::CStr::from_ptr(gl::GetString(gl::VERSION).cast()) }
            .to_str()
            .unwrap();
        assert!(version.starts_with("OpenGL ES 3.1"), "{version}");
        let extensions = unsafe { std::ffi::CStr::from_ptr(gl::GetString(gl::EXTENSIONS).cast()) }
            .to_str()
            .unwrap();
        assert!(
            extensions
                .split_ascii_whitespace()
                .any(|name| name == "GL_ANGLE_webgl_compatibility")
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[gl::VERSION as i64], ""),
            json!("WebGL 2.0 (ANGLE)")
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_essl300_vertex_id_and_special_float_builtins_produce_native_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(
            &mut context,
            gl::VERTEX_SHADER,
            "#version 300 es\nvoid main(){uint i=uint(gl_VertexID);vec2 p=vec2(float((i<<1u)&2u),float(i&2u));gl_Position=vec4(p*2.0-1.0,0,1);}",
        );
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){float v=gl_FragCoord.x;bool bad=isnan(v)||isinf(v);color=bad?vec4(0,0,1,1):vec4(1,0,0,1);}",
        );
        for shader in [vertex, fragment] {
            assert!(
                compiled(&mut context, shader),
                "{}",
                call(&mut context, "getShaderInfoLog", &[shader as i64], "")
            );
        }
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
        call(&mut context, "useProgram", &[program as i64], "");
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}

#[test]
fn webgl2_validates_actual_shader_version_and_does_not_upgrade_webgl1() {
    session::run_native_test(|| {
        let source = "#version 300 es\nvoid main(){gl_Position=vec4(0);}";
        let mut one = WebGl::new(2, 2, Options::default()).unwrap();
        let shader = compile(&mut one, gl::VERTEX_SHADER, source);
        assert!(!compiled(&mut one, shader));
        let mut two = version_two();
        let shader = compile(&mut two, gl::VERTEX_SHADER, source);
        assert!(compiled(&mut two, shader));
        let shader = compile(
            &mut two,
            gl::VERTEX_SHADER,
            "#version 310 es\nvoid main(){gl_Position=vec4(0);}",
        );
        assert!(
            !compiled(&mut two, shader),
            "WebGL2 must not admit GLES3.1 shaders"
        );
        let shader = compile(
            &mut two,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision mediump float;void main(){gl_FragColor=vec4(1);}",
        );
        assert!(
            !compiled(&mut two, shader),
            "ESSL300 has explicit fragment outputs"
        );
    });
}

#[test]
fn webgl2_long_identifier_boundary_preserves_original_uniform_names() {
    session::run_native_test(|| {
        let mut context = version_two();
        for length in [256, 1022, 1023, 1024, 1025] {
            let name = format!("_u{}", "a".repeat(length - 2));
            let source =
                format!("#version 300 es\nuniform vec4 {name};void main(){{gl_Position={name};}}");
            let shader = compile(&mut context, gl::VERTEX_SHADER, &source);
            assert_eq!(
                compiled(&mut context, shader),
                length <= 1024,
                "identifier length {length}: {}",
                call(&mut context, "getShaderInfoLog", &[shader as i64], "")
            );
            if length > 1024 {
                continue;
            }
            let fragment = compile(
                &mut context,
                gl::FRAGMENT_SHADER,
                "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(1);}",
            );
            let program = link(&mut context, shader, fragment);
            assert_eq!(
                call(
                    &mut context,
                    "getProgramParameter",
                    &[program as i64, gl::LINK_STATUS as i64],
                    ""
                ),
                json!(true)
            );
            let location = call(&mut context, "getUniformLocation", &[program as i64], &name);
            assert!(
                location.as_u64().is_some(),
                "exact public uniform name must be usable"
            );
        }
    });
}

#[test]
fn internal_webgl2_creation_rejects_unknown_versions_without_consuming_a_context() {
    let mut contexts = Contexts::default();
    for options in [r#"{"api":"webgl3"}"#, r#"{"api":3}"#, r#"{"api":null}"#] {
        assert!(contexts.create(2, 2, options).is_none());
    }
    let id = contexts
        .create(2, 2, r#"{"api":"webgl2"}"#)
        .expect("closed internal API version");
    assert_eq!(
        tests::command(
            &mut contexts,
            id,
            "getParameter",
            &[gl::VERSION],
            &[],
            "",
            None
        ),
        json!("WebGL 2.0 (ANGLE)")
    );
}
