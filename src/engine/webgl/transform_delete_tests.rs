//! Native name deletion and retained container storage are different lifetimes.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::program;
use super::transform_feedback_tests::{INTERLEAVED, OBJECT, TARGET, buffer, draw, object, shader};
use super::*;

#[test]
fn webgl2_transform_delete_detaches_active_capture_without_ending_it() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let output = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 1);
        call(&mut context, "deleteBuffer", &[output], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e24], ""),
            json!(true)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8e23], ""),
            json!(false)
        );
        assert!(context.objects.get(output as u32, Kind::Buffer).is_err());
        assert_eq!(
            context.dispatch(
                &Command {
                    op: "drawArrays".into(),
                    i: vec![gl::POINTS as i64, 0, 1],
                    f: vec![],
                    text: String::new()
                },
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "endTransformFeedback", &[], "");
        let next = buffer(&mut context, 8);
        call(&mut context, "bindBufferBase", &[TARGET, 0, next], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw(&mut context, 0, 1);
        call(&mut context, "endTransformFeedback", &[], "");
        assert_eq!(
            call(&mut context, "getError", &[], ""),
            json!(gl::INVALID_OPERATION)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

pub(super) fn retained_array(context: &mut WebGl, output: i64) -> i64 {
    let vao = call(context, "createVertexArray", &[], "")
        .as_i64()
        .unwrap();
    call(context, "bindVertexArray", &[vao], "");
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, output],
        "",
    );
    call(
        context,
        "vertexAttribPointer",
        &[0, 2, gl::FLOAT as i64, 0, 0, 0],
        "",
    );
    call(context, "enableVertexAttribArray", &[0], "");
    call(context, "bindVertexArray", &[0], "");
    call(context, "bindBuffer", &[gl::ARRAY_BUFFER as i64, 0], "");
    vao
}

fn draw_checked(context: &mut WebGl, phase: &str) {
    context
        .dispatch(
            &Command {
                op: "drawArrays".into(),
                i: vec![gl::POINTS as i64, 0, 1],
                f: vec![],
                text: String::new(),
            },
            None,
        )
        .unwrap_or_else(|error| panic!("{phase}: {error:#x}"));
}

#[test]
fn webgl2_transform_delete_keeps_gpu_written_storage_in_inactive_containers() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let output = buffer(&mut context, 16);
        let retired_name = context
            .objects
            .get(output as u32, Kind::Buffer)
            .unwrap()
            .native;
        let vao = retained_array(&mut context, output);
        let peer = object(&mut context);
        call(&mut context, "bindTransformFeedback", &[OBJECT, peer], "");
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(&mut context, "bindTransformFeedback", &[OBJECT, 0], "");
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        draw_checked(&mut context, "capture before deleting retained buffer");
        call(&mut context, "deleteBuffer", &[output], "");
        call(&mut context, "endTransformFeedback", &[], "");
        let retained = context.objects.get(output as u32, Kind::Buffer).unwrap();
        assert!(retained.pending_delete && retained.native_deleted);
        assert_eq!(
            context.objects.name(output as u32, Kind::Buffer),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "bindTransformFeedback", &[OBJECT, peer], "");
        assert_eq!(
            call(&mut context, "getIndexedParameter", &[0x8c8f, 0], ""),
            json!(output)
        );
        call(&mut context, "bindTransformFeedback", &[OBJECT, 0], "");
        let fresh = buffer(&mut context, 16);
        let fresh_name = context
            .objects
            .get(fresh as u32, Kind::Buffer)
            .unwrap()
            .native;
        assert_ne!(
            fresh_name, retired_name,
            "retained native allocations must keep their numeric identity reserved"
        );
        let poison = [1f32, 1., 1., 1.].map(f32::to_ne_bytes).concat();
        context
            .dispatch(
                &Command {
                    op: "bufferSubData".into(),
                    i: vec![TARGET, 0],
                    f: vec![],
                    text: String::new(),
                },
                Some(&poison),
            )
            .unwrap();
        program(
            &mut context,
            "#version 300 es\nlayout(location=0) in vec2 data;out float value;void main(){value=data.x;gl_Position=vec4(0,0,0,1);gl_PointSize=4.0;}",
            "#version 300 es\nprecision highp float;in float value;out vec4 color;void main(){color=vec4(value,0,0,1);}",
        );
        call(&mut context, "bindVertexArray", &[vao], "");
        assert_eq!(
            call(
                &mut context,
                "getVertexAttrib",
                &[0, gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING as i64],
                ""
            ),
            json!(output)
        );
        draw_checked(
            &mut context,
            "draw retained capture storage before another deletion",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [128, 0, 0, 255]),
            "inactive VAO reads captured GPU value 0.5, not fresh same-name value 1.0"
        );
        // Deleting another buffer while this VAO is current must not rebuild it
        // by rebinding the numeric name of its already-deleted source buffer.
        call(&mut context, "deleteBuffer", &[fresh], "");
        draw_checked(
            &mut context,
            "draw retained capture storage after another deletion",
        );
        assert!(
            context
                .surface
                .snapshot()
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [128, 0, 0, 255])
        );
        call(&mut context, "deleteTransformFeedback", &[peer], "");
        call(&mut context, "deleteVertexArray", &[vao], "");
        assert!(context.objects.get(output as u32, Kind::Buffer).is_err());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
