use crate::engine::webgl::api_version_tests::{call, version_two};
use crate::engine::webgl::core_uniform_tests::program;
use crate::engine::webgl::*;

const VERTEX: &str = "#version 300 es\nlayout(location=0) in vec2 position;\
    void main(){gl_Position=vec4(position,0,1);}";
const FRAGMENT: &str = "#version 300 es\nprecision mediump float;out vec4 color;\
    void main(){color=vec4(1,0,0,1);}";

fn data(context: &mut WebGl, floats: &[f32]) -> u32 {
    let buffer = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(
        context,
        "bindBuffer",
        &[gl::ARRAY_BUFFER as i64, buffer as i64],
        "",
    );
    replace(context, floats);
    buffer
}

fn replace(context: &mut WebGl, floats: &[f32]) {
    let bytes: Vec<_> = floats.iter().flat_map(|v| v.to_ne_bytes()).collect();
    context
        .dispatch(
            &Command {
                op: "bufferData".into(),
                i: vec![
                    gl::ARRAY_BUFFER as i64,
                    bytes.len() as i64,
                    gl::DYNAMIC_DRAW as i64,
                ],
                f: vec![],
                text: String::new(),
            },
            Some(&bytes),
        )
        .unwrap();
}

fn pointer(context: &mut WebGl, index: i64, width: i64, stride: i64, offset: i64) {
    call(
        context,
        "vertexAttribPointer",
        &[index, width, gl::FLOAT as i64, 0, stride, offset],
        "",
    );
    call(context, "enableVertexAttribArray", &[index], "");
}

fn draw(context: &mut WebGl) -> Result<Value> {
    context.dispatch(
        &Command {
            op: "drawArrays".into(),
            i: vec![gl::TRIANGLES as i64, 0, 3],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

#[test]
fn repeated_native_draws_scan_linked_inputs_once_and_preserve_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        let owner = program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        for _ in 0..200 {
            assert_eq!(draw(&mut context), Ok(Value::Null));
        }
        assert_eq!(context.attribute_reflection.native_scans, 1);
        let generation = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        assert_eq!(
            context.attribute_reflection.lookup(owner, generation),
            Some(1)
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
    });
}

#[test]
fn cached_inputs_never_cache_mutable_buffer_capacity_stride_or_offsets() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        replace(&mut context, &[0., 0.]);
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        replace(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        assert!(draw(&mut context).is_ok());
        pointer(&mut context, 0, 2, 16, 0);
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        pointer(&mut context, 0, 2, 0, 4);
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 1);
    });
}

#[test]
fn cache_hits_still_require_buffers_for_inactive_enabled_arrays() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        call(&mut context, "enableVertexAttribArray", &[1], "");
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        // Inactive ranges do not need vertices, but still need a bound buffer.
        data(&mut context, &[]);
        pointer(&mut context, 1, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 1);
    });
}

#[test]
fn vertex_array_switches_keep_program_metadata_but_use_the_new_array_bounds() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        let short = call(&mut context, "createVertexArray", &[], "")
            .as_i64()
            .unwrap();
        call(&mut context, "bindVertexArray", &[short], "");
        data(&mut context, &[0., 0.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        call(&mut context, "bindVertexArray", &[0], "");
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 1);
    });
}

#[test]
fn bind_attribute_location_only_changes_cached_reflection_after_real_relink() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source =
            "#version 300 es\nin vec2 position;void main(){gl_Position=vec4(position,0,1);}";
        let owner = program(&mut context, source, FRAGMENT);
        assert!(context.validate_attributes(0).is_ok());
        let old = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        assert_eq!(context.attribute_reflection.lookup(owner, old), Some(1));
        call(
            &mut context,
            "bindAttribLocation",
            &[owner as i64, 5],
            "position",
        );
        assert!(context.validate_attributes(0).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 1);
        call(&mut context, "linkProgram", &[owner as i64], "");
        call(&mut context, "useProgram", &[owner as i64], "");
        assert!(context.validate_attributes(0).is_ok());
        let generation = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        assert_ne!(old, generation);
        assert_eq!(context.attribute_reflection.lookup(owner, old), None);
        assert_eq!(
            context.attribute_reflection.lookup(owner, generation),
            Some(1 << 5)
        );
        assert_eq!(context.attribute_reflection.native_scans, 2);
    });
}

#[test]
fn matrix_reflection_checks_every_real_native_column() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = "#version 300 es\nlayout(location=0) in vec2 position;\
            layout(location=2) in mat3 basis;void main(){gl_Position=vec4(basis*vec3(position,1),1);}";
        let owner = program(&mut context, vertex, FRAGMENT);
        let buffer = data(&mut context, &[1.; 9]);
        for index in 2..5 {
            pointer(&mut context, index, 3, 0, 0);
        }
        assert!(context.validate_attributes(2).is_ok());
        let generation = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        assert_eq!(
            context.attribute_reflection.lookup(owner, generation),
            Some(0b11101)
        );
        // A bad final matrix column must not disappear from the reflected mask.
        pointer(&mut context, 4, 3, 0, 4);
        assert_eq!(context.validate_attributes(2), Err(gl::INVALID_OPERATION));
        assert_eq!(
            context
                .objects
                .get(buffer, Kind::Buffer)
                .unwrap()
                .bytes
                .len(),
            36
        );
        assert_eq!(context.attribute_reflection.native_scans, 1);
    });
}

#[test]
fn instanced_attribute_divisors_are_checked_after_each_cached_lookup() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = "#version 300 es\nlayout(location=0) in vec2 position;\
            layout(location=1) in vec2 shift;void main(){gl_Position=vec4(position+shift,0,1);}";
        program(&mut context, vertex, FRAGMENT);
        data(&mut context, &[0.; 6]);
        pointer(&mut context, 0, 2, 0, 0);
        data(&mut context, &[0.; 4]);
        pointer(&mut context, 1, 2, 0, 0);
        call(&mut context, "vertexAttribDivisor", &[1, 1], "");
        assert!(context.validate_instance_attributes(2, 2, true).is_ok());
        assert_eq!(
            context.validate_instance_attributes(2, 3, true),
            Err(gl::INVALID_OPERATION)
        );
        call(&mut context, "vertexAttribDivisor", &[1, 2], "");
        assert!(context.validate_instance_attributes(2, 3, true).is_ok());
        call(&mut context, "vertexAttribDivisor", &[1, 0], "");
        assert_eq!(
            context.validate_instance_attributes(2, 3, true),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(context.attribute_reflection.native_scans, 1);
    });
}

#[test]
fn failed_relink_discards_cached_inputs_and_a_later_real_link_restores_them() {
    session::run_native_test(|| {
        let mut context = version_two();
        let owner = program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        let generation = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        let shaders = context.objects.attached_shaders(owner).unwrap().to_vec();
        for shader in &shaders {
            call(
                &mut context,
                "detachShader",
                &[owner as i64, *shader as i64],
                "",
            );
        }
        call(&mut context, "linkProgram", &[owner as i64], "");
        assert_eq!(context.attribute_reflection.lookup(owner, generation), None);
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[owner as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(false)
        );
        assert_eq!(draw(&mut context), Err(gl::INVALID_OPERATION));
        for shader in shaders {
            call(
                &mut context,
                "attachShader",
                &[owner as i64, shader as i64],
                "",
            );
        }
        call(&mut context, "linkProgram", &[owner as i64], "");
        call(&mut context, "useProgram", &[owner as i64], "");
        assert!(draw(&mut context).is_ok());
        let current = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        assert_ne!(generation, current);
        assert_eq!(context.attribute_reflection.lookup(owner, current), Some(1));
    });
}

#[test]
fn deleting_a_current_program_keeps_its_actual_executable_until_final_release() {
    session::run_native_test(|| {
        let mut context = version_two();
        let owner = program(&mut context, VERTEX, FRAGMENT);
        data(&mut context, &[-1., -1., 3., -1., -1., 3.]);
        pointer(&mut context, 0, 2, 0, 0);
        assert!(draw(&mut context).is_ok());
        call(&mut context, "deleteProgram", &[owner as i64], "");
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 1);
        call(&mut context, "useProgram", &[0], "");
        assert!(context.objects.get(owner, Kind::Program).is_err());
        let replacement = program(&mut context, VERTEX, FRAGMENT);
        assert_ne!(replacement, owner);
        assert!(draw(&mut context).is_ok());
        assert_eq!(context.attribute_reflection.native_scans, 2);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
