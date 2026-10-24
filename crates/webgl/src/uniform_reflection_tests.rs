//! Actual native locations, types and pixels remain authoritative with metadata reuse.
use super::{
    api_version_tests::{call, compile, link, version_two},
    core_uniform_tests::{VERTEX, program},
    *,
};

fn location(context: &mut WebGl, owner: u32, name: &str) -> u32 {
    call(context, "getUniformLocation", &[owner as i64], name)
        .as_u64()
        .expect("actual active uniform location") as u32
}

fn upload(context: &mut WebGl, op: &str, id: u32, values: &[f64]) -> Result<Value> {
    context.dispatch(
        &Command {
            op: op.into(),
            i: vec![id as i64],
            f: values.to_vec(),
            text: String::new(),
        },
        None,
    )
}

#[test]
fn distinct_uniform_locations_share_one_real_scan_and_still_render() {
    session::run_native_test(|| {
        let mut context = version_two();
        let declarations = (0..64)
            .map(|index| format!("uniform vec4 value{index};"))
            .collect::<String>();
        let additions = (0..64)
            .map(|index| format!("sum+=value{index};"))
            .collect::<String>();
        let fragment = format!(
            "#version 300 es\nprecision highp float;{declarations}out vec4 color;void main(){{vec4 sum=vec4(0);{additions}color=sum/64.;}}"
        );
        let owner = program(&mut context, VERTEX, &fragment);
        for index in 0..64 {
            let id = location(&mut context, owner, &format!("value{index}"));
            assert_eq!(
                context.objects.uniform_location(id).unwrap().uniform_type,
                gl::FLOAT_VEC4
            );
            assert_eq!(
                upload(&mut context, "uniform4f", id, &[0.25, 1.0, 0.0, 1.0]),
                Ok(Value::Null)
            );
        }
        assert_eq!(context.uniform_reflection.native_scans, 1);
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
                .all(|pixel| pixel == [64, 255, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn successful_relink_discards_old_types_and_location_generations() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;uniform vec4 value;out vec4 color;void main(){color=value;}",
        );
        let owner = link(&mut context, vertex, fragment);
        call(&mut context, "useProgram", &[owner as i64], "");
        let old = location(&mut context, owner, "value");
        assert_eq!(context.uniform_reflection.native_scans, 1);
        call(
            &mut context,
            "shaderSource",
            &[fragment as i64],
            "#version 300 es\nprecision highp float;uniform vec3 value;out vec4 color;void main(){color=vec4(value,1);}",
        );
        call(&mut context, "compileShader", &[fragment as i64], "");
        call(&mut context, "linkProgram", &[owner as i64], "");
        let new = location(&mut context, owner, "value");
        assert_ne!(old, new);
        assert!(context.objects.uniform_location(old).is_err());
        assert_eq!(
            context.objects.uniform_location(new).unwrap().uniform_type,
            gl::FLOAT_VEC3
        );
        assert_eq!(context.uniform_reflection.native_scans, 2);
        assert_eq!(
            upload(&mut context, "uniform4f", new, &[0.0, 0.0, 0.0, 1.0]),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            call(&mut context, "getError", &[], ""),
            json!(gl::INVALID_OPERATION)
        );
        assert_eq!(
            upload(&mut context, "uniform3f", new, &[0.25, 1.0, 0.0]),
            Ok(Value::Null)
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn failed_relink_cannot_supply_cached_reflection_from_the_previous_executable() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;uniform float value;out vec4 color;void main(){color=vec4(value);}",
        );
        let owner = link(&mut context, vertex, fragment);
        let old = location(&mut context, owner, "value");
        let generation = context
            .objects
            .get(owner, Kind::Program)
            .unwrap()
            .generation;
        call(&mut context, "shaderSource", &[fragment as i64], "invalid");
        call(&mut context, "compileShader", &[fragment as i64], "");
        call(&mut context, "linkProgram", &[owner as i64], "");
        assert_eq!(
            context
                .uniform_reflection
                .lookup(owner, generation, "value"),
            None
        );
        assert!(context.objects.uniform_location(old).is_err());
        assert_eq!(
            context.dispatch(
                &Command {
                    op: "getUniformLocation".into(),
                    i: vec![owner as i64],
                    f: vec![],
                    text: "value".into(),
                },
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(context.uniform_reflection.native_scans, 1);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn nested_struct_array_fields_and_matrix_types_remain_distinct() {
    session::run_native_test(|| {
        let mut context = version_two();
        let owner = program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;struct Light{vec3 tint;float power;};uniform Light lights[2];uniform mat2 basis;out vec4 color;void main(){color=vec4(lights[0].tint*lights[0].power+lights[1].tint*lights[1].power,basis[0][0]+basis[1][1]);}",
        );
        for index in 0..2 {
            let tint = location(&mut context, owner, &format!("lights[{index}].tint"));
            let power = location(&mut context, owner, &format!("lights[{index}].power"));
            assert_eq!(
                context.objects.uniform_location(tint).unwrap().uniform_type,
                gl::FLOAT_VEC3
            );
            assert_eq!(
                context
                    .objects
                    .uniform_location(power)
                    .unwrap()
                    .uniform_type,
                gl::FLOAT
            );
        }
        let basis = location(&mut context, owner, "basis");
        assert_eq!(
            context
                .objects
                .uniform_location(basis)
                .unwrap()
                .uniform_type,
            gl::FLOAT_MAT2
        );
        assert_eq!(context.uniform_reflection.native_scans, 1);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn changing_or_compiling_attached_source_does_not_change_linked_metadata() {
    session::run_native_test(|| {
        let mut context = version_two();
        let vertex = compile(&mut context, gl::VERTEX_SHADER, VERTEX);
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;uniform float first;uniform vec3 second;out vec4 color;void main(){color=vec4(second,first);}",
        );
        let owner = link(&mut context, vertex, fragment);
        location(&mut context, owner, "first");
        call(
            &mut context,
            "shaderSource",
            &[fragment as i64],
            "#version 300 es\nprecision highp float;uniform int second;out vec4 color;void main(){color=vec4(float(second));}",
        );
        call(&mut context, "compileShader", &[fragment as i64], "");
        let second = location(&mut context, owner, "second");
        assert_eq!(
            context
                .objects
                .uniform_location(second)
                .unwrap()
                .uniform_type,
            gl::FLOAT_VEC3
        );
        assert_eq!(context.uniform_reflection.native_scans, 1);
        assert_eq!(
            call(
                &mut context,
                "getUniformLocation",
                &[owner as i64],
                "missing"
            ),
            Value::Null
        );
        assert_eq!(context.uniform_reflection.native_scans, 1);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
