//! Typed attribute inputs are accepted only when the actual GPU shader contract agrees.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::program;
use super::*;

fn input(context: &mut WebGl, bytes: &[u8]) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, id as i64],
        "",
    );
    let command = Command {
        op: "bufferData".into(),
        i: vec![
            gl::ARRAY_BUFFER as i64,
            bytes.len() as i64,
            gl::STATIC_DRAW as i64,
        ],
        f: vec![],
        text: String::new(),
    };
    assert_eq!(context.dispatch(&command, Some(bytes)), Ok(Value::Null));
    id
}

fn attribute_program(context: &mut WebGl, integer: Option<bool>) -> (u32, i64) {
    let (kind, varying) = match integer {
        Some(true) => ("uvec4", "flat"),
        Some(false) => ("ivec4", "flat"),
        None => ("vec4", ""),
    };
    let vertex = format!(
        "#version 300 es\nin {kind} data;{varying} out {kind} color;void main(){{int i=gl_VertexID;vec2 p=vec2(float((i<<1)&2),float(i&2));gl_Position=vec4(p*2.0-1.0,0,1);color=data;}}"
    );
    let expression = match integer {
        Some(true) => "vec4(vec3(color)/255.0,1)",
        Some(false) => "color.x<0?vec4(1,0,0,1):vec4(0,0,1,1)",
        None => "color",
    };
    let fragment = format!(
        "#version 300 es\nprecision highp float;{varying} in highp {kind} color;out vec4 result;void main(){{result={expression};}}"
    );
    let program = program(context, &vertex, &fragment);
    let index = call(context, "getAttribLocation", &[program as i64], "data")
        .as_i64()
        .unwrap();
    (program, index)
}

fn query(context: &mut WebGl, index: i64, pname: u32) -> Value {
    call(context, "getVertexAttrib", &[index, pname as i64], "")
}

fn red(context: &mut WebGl) {
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert!(
        context
            .surface
            .snapshot()
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel == [255, 0, 0, 255])
    );
    assert_eq!(call(context, "getError", &[], ""), json!(0));
}

#[test]
fn webgl2_integer_vertex_pointers_render_signed_and_unsigned_buffer_values() {
    session::run_native_test(|| {
        let mut context = version_two();
        for unsigned in [true, false] {
            let (_, index) = attribute_program(&mut context, Some(unsigned));
            let bytes = if unsigned {
                [255, 0, 0, 255]
            } else {
                [255, 0, 0, 1]
            };
            let buffer = input(&mut context, &bytes.repeat(3));
            let kind = if unsigned {
                gl::UNSIGNED_BYTE
            } else {
                gl::BYTE
            };
            call(
                &mut context,
                "vertexAttribIPointer",
                &[index, 4, kind as i64, 0, 0],
                "",
            );
            call(&mut context, "enableVertexAttribArray", &[index], "");
            assert_eq!(query(&mut context, index, 0x88fd), json!(true));
            assert_eq!(
                query(&mut context, index, gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING),
                json!(buffer)
            );
            assert_eq!(
                query(&mut context, index, gl::VERTEX_ATTRIB_ARRAY_SIZE),
                json!(4)
            );
            assert_eq!(
                query(&mut context, index, gl::VERTEX_ATTRIB_ARRAY_NORMALIZED),
                json!(false)
            );
            red(&mut context);
        }
    });
}

#[test]
fn webgl2_integer_constants_preserve_signedness_and_are_not_vertex_array_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (_, index) = attribute_program(&mut context, Some(true));
        call(
            &mut context,
            "vertexAttribI4ui",
            &[index, u32::MAX as i64, 0x80000000, 7, 9],
            "",
        );
        let expected = json!({"kind":"uint","values":[u32::MAX,0x80000000u32,7,9]});
        assert_eq!(
            query(&mut context, index, gl::CURRENT_VERTEX_ATTRIB),
            expected
        );
        let vao = call(&mut context, "createVertexArray", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindVertexArray", &[vao], "");
        assert_eq!(
            query(&mut context, index, gl::CURRENT_VERTEX_ATTRIB),
            expected
        );
        call(
            &mut context,
            "vertexAttribI4i",
            &[index, i32::MIN as i64, i32::MAX as i64, -7, 9],
            "",
        );
        assert_eq!(
            query(&mut context, index, gl::CURRENT_VERTEX_ATTRIB),
            json!({"kind":"int","values":[i32::MIN,i32::MAX,-7,9]})
        );
        call(&mut context, "bindVertexArray", &[0], "");
        assert_eq!(
            query(&mut context, index, gl::CURRENT_VERTEX_ATTRIB)["kind"],
            json!("int")
        );
        let command = Command {
            op: "vertexAttrib4f".into(),
            i: vec![index],
            f: vec![1., 2., 3., 4.],
            text: String::new(),
        };
        assert_eq!(context.dispatch(&command, None), Ok(Value::Null));
        assert_eq!(
            query(&mut context, index, gl::CURRENT_VERTEX_ATTRIB),
            json!({"kind":"float","values":[1.,2.,3.,4.]})
        );
    });
}

#[test]
fn webgl2_half_float_and_packed_vertex_inputs_use_the_correct_element_byte_width() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (_, index) = attribute_program(&mut context, None);
        let half = [0x3c00u16, 0, 0, 0x3c00]
            .into_iter()
            .flat_map(u16::to_ne_bytes)
            .collect::<Vec<_>>();
        for (kind, normalized, bytes) in [
            (0x140b, 0, half),
            (0x8368, 1, 0xc00003ffu32.to_ne_bytes().to_vec()),
            (0x8d9f, 1, 0x400001ffu32.to_ne_bytes().to_vec()),
        ] {
            input(&mut context, &bytes.repeat(3));
            call(
                &mut context,
                "vertexAttribPointer",
                &[index, 4, kind, normalized, 0, 0],
                "",
            );
            call(&mut context, "enableVertexAttribArray", &[index], "");
            assert_eq!(
                query(&mut context, index, gl::VERTEX_ATTRIB_ARRAY_SIZE),
                json!(4)
            );
            assert_eq!(query(&mut context, index, 0x88fd), json!(false));
            red(&mut context);
        }
    });
}

#[test]
fn webgl2_vertex_array_rebuild_retains_integer_formats_after_buffer_deletion() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (_, index) = attribute_program(&mut context, Some(true));
        let deleted = input(&mut context, &[255, 0, 0, 255]);
        call(
            &mut context,
            "vertexAttribIPointer",
            &[index, 4, gl::UNSIGNED_BYTE as i64, 0, 0],
            "",
        );
        call(&mut context, "deleteBuffer", &[deleted as i64], "");
        assert_eq!(query(&mut context, index, 0x88fd), json!(true));
        assert_eq!(
            query(&mut context, index, gl::VERTEX_ATTRIB_ARRAY_BUFFER_BINDING),
            json!(0)
        );
        input(&mut context, &[255, 0, 0, 255].repeat(3));
        call(
            &mut context,
            "vertexAttribIPointer",
            &[index, 4, gl::UNSIGNED_BYTE as i64, 0, 0],
            "",
        );
        call(&mut context, "enableVertexAttribArray", &[index], "");
        red(&mut context);
    });
}

#[test]
fn webgl2_invalid_integer_pointer_does_not_replace_the_current_format() {
    session::run_native_test(|| {
        let mut context = version_two();
        input(&mut context, &[0; 16]);
        call(
            &mut context,
            "vertexAttribIPointer",
            &[0, 4, gl::UNSIGNED_INT as i64, 0, 0],
            "",
        );
        for (arguments, error) in [
            (vec![0, 4, gl::FLOAT as i64, 0, 0], gl::INVALID_ENUM),
            (vec![0, 0, gl::INT as i64, 0, 0], gl::INVALID_VALUE),
            (vec![0, 4, gl::INT as i64, 256, 0], gl::INVALID_VALUE),
            (vec![0, 4, gl::INT as i64, 0, 1], gl::INVALID_OPERATION),
        ] {
            let command = Command {
                op: "vertexAttribIPointer".into(),
                i: arguments,
                f: vec![],
                text: String::new(),
            };
            assert_eq!(context.dispatch(&command, None), Err(error));
            assert_eq!(
                query(&mut context, 0, gl::VERTEX_ATTRIB_ARRAY_TYPE),
                json!(gl::UNSIGNED_INT)
            );
            assert_eq!(query(&mut context, 0, 0x88fd), json!(true));
        }
    });
}
