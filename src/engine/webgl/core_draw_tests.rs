//! WebGL2 core draw behavior uses real geometry, not renamed extension probes.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;
const FRAGMENT: &str =
    "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(1,0,0,1);}";

#[test]
fn webgl2_range_hints_do_not_reject_indices_outside_the_hint() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        buffer(&mut context, gl::ELEMENT_ARRAY_BUFFER, &[0, 1, 2]);
        call(
            &mut context,
            "drawRangeElements",
            &[gl::TRIANGLES as i64, 1, 1, 3, gl::UNSIGNED_BYTE as i64, 0],
            "",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        for (start, end, count, offset, error) in [
            (2, 1, 3, 0, gl::INVALID_VALUE),
            (0, 2, -1, 0, gl::INVALID_VALUE),
            (0, 2, 4, 0, gl::INVALID_OPERATION),
            (0, 2, 3, -1, gl::INVALID_VALUE),
        ] {
            let command = Command {
                op: "drawRangeElements".into(),
                i: vec![
                    gl::TRIANGLES as i64,
                    start,
                    end,
                    count,
                    gl::UNSIGNED_BYTE as i64,
                    offset,
                ],
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(error));
        }
    });
}

fn buffer(context: &mut WebGl, target: u32, bytes: &[u8]) -> u32 {
    let buffer = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(context, "bindBuffer", &[target as i64, buffer as i64], "");
    let command = Command {
        op: "bufferData".into(),
        i: vec![target as i64, bytes.len() as i64, gl::STATIC_DRAW as i64],
        f: vec![],
        text: String::new(),
    };
    assert_eq!(context.dispatch(&command, Some(bytes)), Ok(Value::Null));
    buffer
}

#[test]
fn webgl2_core_instanced_vertex_id_draw_needs_no_webgl1_extension() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        assert!(context.vertex_arrays.default_native != 0);
        assert_eq!(call(&mut context, "getParameter", &[0x85b5], ""), json!(0));
        call(
            &mut context,
            "drawArraysInstanced",
            &[gl::TRIANGLES as i64, 0, 3, 2],
            "",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        let extensions = call(&mut context, "supportedExtensions", &[], "");
        for removed in [
            "ANGLE_instanced_arrays",
            "OES_vertex_array_object",
            "OES_element_index_uint",
            "OES_standard_derivatives",
            "EXT_frag_depth",
            "EXT_shader_texture_lod",
            "WEBGL_draw_buffers",
        ] {
            assert!(
                !extensions.as_array().unwrap().contains(&json!(removed)),
                "{removed} is core, not an extension"
            );
        }
    });
}

#[test]
fn webgl2_core_vertex_arrays_keep_element_bindings_and_native_lifetimes() {
    session::run_native_test(|| {
        let mut context = version_two();
        let first = call(&mut context, "createVertexArray", &[], "")
            .as_u64()
            .unwrap() as u32;
        let second = call(&mut context, "createVertexArray", &[], "")
            .as_u64()
            .unwrap() as u32;
        assert_eq!(
            call(&mut context, "isVertexArray", &[first as i64], ""),
            json!(false)
        );
        call(&mut context, "bindVertexArray", &[first as i64], "");
        assert_eq!(
            call(&mut context, "isVertexArray", &[first as i64], ""),
            json!(true)
        );
        let indices = buffer(&mut context, gl::ELEMENT_ARRAY_BUFFER, &[0, 1, 2]);
        call(&mut context, "bindVertexArray", &[second as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::ELEMENT_ARRAY_BUFFER_BINDING as i64],
                ""
            ),
            Value::Null
        );
        call(&mut context, "bindVertexArray", &[first as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::ELEMENT_ARRAY_BUFFER_BINDING as i64],
                ""
            ),
            json!(indices)
        );
        let native = context
            .objects
            .get(first, Kind::VertexArray)
            .unwrap()
            .native;
        call(&mut context, "deleteVertexArray", &[first as i64], "");
        assert_eq!(call(&mut context, "getParameter", &[0x85b5], ""), json!(0));
        assert_eq!(
            call(&mut context, "isVertexArray", &[first as i64], ""),
            json!(false)
        );
        assert_eq!(
            unsafe { (context.core.as_ref().unwrap().is_array)(native) },
            0
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_fixed_restart_markers_do_not_expand_attribute_ranges() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        for (kind, size) in [
            (gl::UNSIGNED_BYTE, 1),
            (gl::UNSIGNED_SHORT, 2),
            (gl::UNSIGNED_INT, 4),
        ] {
            let mut bytes = Vec::new();
            for index in [0u32, 1, 2, u32::MAX, 0, 1, 2] {
                bytes.extend(&index.to_ne_bytes()[..size]);
            }
            buffer(&mut context, gl::ELEMENT_ARRAY_BUFFER, &bytes);
            call(
                &mut context,
                "drawElements",
                &[gl::TRIANGLES as i64, 7, kind as i64, 0],
                "",
            );
            assert!(
                context
                    .surface
                    .snapshot()
                    .unwrap()
                    .chunks_exact(4)
                    .all(|pixel| pixel == [255, 0, 0, 255])
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
            call(
                &mut context,
                "drawElementsInstanced",
                &[gl::TRIANGLES as i64, 7, kind as i64, 0, 2],
                "",
            );
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn webgl2_restart_only_draw_does_not_fetch_active_attribute_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        let program = program(
            &mut context,
            "#version 300 es\nin vec2 p;void main(){gl_Position=vec4(p,0,1);}",
            FRAGMENT,
        );
        let index = call(&mut context, "getAttribLocation", &[program as i64], "p")
            .as_u64()
            .unwrap();
        // One vertex is sufficient even though the byte-sized restart marker is
        // 255. Native WebGL validation still requires allocated attribute storage.
        buffer(&mut context, gl::ARRAY_BUFFER, &[0; 8]);
        call(
            &mut context,
            "vertexAttribPointer",
            &[index as i64, 2, gl::FLOAT as i64, 0, 0, 0],
            "",
        );
        call(&mut context, "enableVertexAttribArray", &[index as i64], "");
        buffer(&mut context, gl::ELEMENT_ARRAY_BUFFER, &[255, 255, 255]);
        call(
            &mut context,
            "drawElements",
            &[gl::TRIANGLES as i64, 3, gl::UNSIGNED_BYTE as i64, 0],
            "",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .iter()
                .all(|byte| *byte == 0)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl1_cannot_call_core_names_after_enabling_its_extensions() {
    let (mut contexts, id) = tests::context();
    for extension in [
        "ANGLE_instanced_arrays",
        "OES_vertex_array_object",
        "WEBGL_draw_buffers",
    ] {
        assert_eq!(
            tests::command(
                &mut contexts,
                id,
                "enableExtension",
                &[],
                &[],
                extension,
                None
            ),
            json!(true)
        );
    }
    for (operation, arguments) in [
        ("createVertexArray", vec![]),
        ("bindVertexArray", vec![0]),
        ("vertexAttribDivisor", vec![0, 1]),
        ("drawArraysInstanced", vec![gl::TRIANGLES, 0, 0, 0]),
        ("drawBuffers", vec![gl::BACK]),
    ] {
        tests::command(&mut contexts, id, operation, &arguments, &[], "", None);
        assert_eq!(
            tests::command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
    }
}
