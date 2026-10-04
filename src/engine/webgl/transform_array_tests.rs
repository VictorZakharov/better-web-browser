//! Array capture is provided by ANGLE, with the WebGL2 shader language boundary intact.
use super::api_version_tests::{call, compile, compiled, link, version_two};
use super::transform_feedback_tests::{
    INTERLEAVED, SEPARATE, TARGET, assert_float_capture, buffer, draw, read,
};
use super::*;

fn program(context: &mut WebGl, body: &str, names: &str, mode: i64) -> u32 {
    let source = format!("#version 300 es\n{body}");
    let vertex = compile(context, gl::VERTEX_SHADER, &source);
    assert!(
        compiled(context, vertex),
        "{}",
        call(context, "getShaderInfoLog", &[vertex as i64], "")
    );
    let fragment = compile(
        context,
        gl::FRAGMENT_SHADER,
        "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0);}",
    );
    assert!(compiled(context, fragment));
    let id = link(context, vertex, fragment);
    call(
        context,
        "transformFeedbackVaryings",
        &[id as i64, mode],
        names,
    );
    call(context, "linkProgram", &[id as i64], "");
    id
}

fn linked(context: &mut WebGl, id: u32) -> bool {
    call(
        context,
        "getProgramParameter",
        &[id as i64, gl::LINK_STATUS as i64],
        "",
    ) == json!(true)
}

fn use_linked(context: &mut WebGl, id: u32) {
    assert!(
        linked(context, id),
        "{}",
        call(context, "getProgramInfoLog", &[id as i64], "")
    );
    call(context, "useProgram", &[id as i64], "");
}

fn capture(context: &mut WebGl, count: i64) {
    call(context, "enable", &[0x8c89], "");
    call(context, "beginTransformFeedback", &[gl::POINTS as i64], "");
    draw(context, 0, count);
    call(context, "endTransformFeedback", &[], "");
}

const VECTORS: &str = "out vec2 values[3];void main(){float n=float(gl_VertexID)*10.0;values[0]=vec2(n+1.0,n+2.0);values[1]=vec2(n+3.0,n+4.0);values[2]=vec2(n+5.0,n+6.0);gl_Position=vec4(0,0,0,1);gl_PointSize=1.0;}";

#[test]
fn webgl2_transform_array_whole_capture_preserves_vertex_and_element_order() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = program(&mut context, VECTORS, r#"["values"]"#, INTERLEAVED);
        use_linked(&mut context, id);
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[id as i64, 0],
                ""
            ),
            json!({"name":"values","size":3,"type":gl::FLOAT_VEC2})
        );
        let output = buffer(&mut context, 64);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        capture(&mut context, 2);
        let bytes = read(&mut context, output, 64);
        assert_float_capture(
            &bytes[..48],
            &[1., 2., 3., 4., 5., 6., 11., 12., 13., 14., 15., 16.],
        );
        assert_eq!(&bytes[48..], &[0; 16]);
    });
}

#[test]
fn webgl2_transform_array_element_capture_uses_requested_not_declaration_order() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = program(
            &mut context,
            VECTORS,
            r#"["values[2]","values[0]"]"#,
            INTERLEAVED,
        );
        use_linked(&mut context, id);
        for (index, name) in ["values[2]", "values[0]"].into_iter().enumerate() {
            assert_eq!(
                call(
                    &mut context,
                    "getTransformFeedbackVarying",
                    &[id as i64, index as i64],
                    ""
                ),
                json!({"name":name,"size":1,"type":gl::FLOAT_VEC2})
            );
        }
        let output = buffer(&mut context, 32);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        capture(&mut context, 2);
        assert_float_capture(
            &read(&mut context, output, 32),
            &[5., 6., 1., 2., 15., 16., 11., 12.],
        );
    });
}

#[test]
fn webgl2_transform_array_separate_buffers_keep_independent_ranges() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = program(
            &mut context,
            VECTORS,
            r#"["values[1]","values[2]"]"#,
            SEPARATE,
        );
        use_linked(&mut context, id);
        let first = buffer(&mut context, 32);
        let second = buffer(&mut context, 40);
        call(
            &mut context,
            "bindBufferRange",
            &[TARGET, 0, first, 8, 16],
            "",
        );
        call(
            &mut context,
            "bindBufferRange",
            &[TARGET, 1, second, 16, 16],
            "",
        );
        capture(&mut context, 2);
        let bytes = read(&mut context, first, 32);
        assert_float_capture(&bytes[8..24], &[3., 4., 13., 14.]);
        assert!(bytes[..8].iter().chain(&bytes[24..]).all(|b| *b == 0));
        let bytes = read(&mut context, second, 40);
        assert_float_capture(&bytes[16..32], &[5., 6., 15., 16.]);
        assert!(bytes[..16].iter().chain(&bytes[32..]).all(|b| *b == 0));
    });
}

#[test]
fn webgl2_transform_array_invalid_or_overlapping_selections_fail_link() {
    session::run_native_test(|| {
        let mut context = version_two();
        for names in [
            r#"["values","values[0]"]"#,
            r#"["values[1]","values[1]"]"#,
            r#"["values[3]"]"#,
            r#"["values[-1]"]"#,
            r#"["missing"]"#,
        ] {
            let id = program(&mut context, VECTORS, names, INTERLEAVED);
            assert!(!linked(&mut context, id), "accepted {names}");
            assert!(
                !call(&mut context, "getProgramInfoLog", &[id as i64], "")
                    .as_str()
                    .unwrap()
                    .is_empty()
            );
            call(&mut context, "deleteProgram", &[id as i64], "");
        }
    });
}

#[test]
fn webgl2_transform_array_capture_preserves_full_signed_and_unsigned_bits() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = program(
            &mut context,
            "flat out uint unsignedValues[2];flat out int signedValues[2];void main(){unsignedValues[0]=4294967295u;unsignedValues[1]=2147483648u; signedValues[0]=int(0x80000000u);signedValues[1]=2147483647;gl_Position=vec4(0,0,0,1);gl_PointSize=1.0;}",
            r#"["unsignedValues","signedValues"]"#,
            INTERLEAVED,
        );
        use_linked(&mut context, id);
        let output = buffer(&mut context, 16);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        capture(&mut context, 1);
        let expected = [u32::MAX, 0x80000000, 0x80000000, 0x7fffffff]
            .map(u32::to_ne_bytes)
            .concat();
        assert_eq!(read(&mut context, output, 16), expected);
    });
}

#[test]
fn webgl2_transform_array_matrix_output_is_column_major() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = program(
            &mut context,
            "out mat2 matrices[2];void main(){matrices[0]=mat2(1,2,3,4);matrices[1]=mat2(5,6,7,8);gl_Position=vec4(0,0,0,1);gl_PointSize=1.0;}",
            r#"["matrices"]"#,
            INTERLEAVED,
        );
        use_linked(&mut context, id);
        assert_eq!(
            call(
                &mut context,
                "getTransformFeedbackVarying",
                &[id as i64, 0],
                ""
            ),
            json!({"name":"matrices","size":2,"type":gl::FLOAT_MAT2})
        );
        let output = buffer(&mut context, 32);
        call(&mut context, "bindBufferBase", &[TARGET, 0, output], "");
        capture(&mut context, 1);
        assert_float_capture(
            &read(&mut context, output, 32),
            &[1., 2., 3., 4., 5., 6., 7., 8.],
        );
    });
}
