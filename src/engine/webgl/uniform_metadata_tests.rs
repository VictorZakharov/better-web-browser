//! Real GLES locations must not crowd shaders out of the GPU object registry.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

#[test]
fn uniform_metadata_over_thousand_locations_still_allows_new_shader() {
    session::run_native_test(|| {
        let mut context = version_two();
        let fragment = "#version 300 es\nprecision highp float;uniform float data[300];out vec4 color;void main(){float sum=0.0;for(int i=0;i<300;i++){sum+=data[i];}color=vec4(sum,0,0,1);}";
        let mut first = None;
        for _ in 0..4 {
            let owner = program(&mut context, VERTEX, fragment);
            for index in 0..300 {
                let name = format!("data[{index}]");
                let id = call(&mut context, "getUniformLocation", &[owner as i64], &name)
                    .as_u64()
                    .expect("active array element") as u32;
                first.get_or_insert((owner, id));
                assert_eq!(
                    call(&mut context, "getUniformLocation", &[owner as i64], &name),
                    json!(id)
                );
            }
        }
        let shader = call(
            &mut context,
            "createShader",
            &[gl::VERTEX_SHADER as i64],
            "",
        );
        assert!(shader.as_u64().is_some_and(|id| id != 0));
        let (owner, old) = first.unwrap();
        call(&mut context, "linkProgram", &[owner as i64], "");
        assert!(context.objects.uniform_location(old).is_err());
        let new = call(
            &mut context,
            "getUniformLocation",
            &[owner as i64],
            "data[0]",
        )
        .as_u64()
        .unwrap() as u32;
        assert_ne!(old, new);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn uniform_metadata_program_retirement_waits_for_current_reference() {
    session::run_native_test(|| {
        let mut context = version_two();
        let owner = program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;uniform float data;out vec4 color;void main(){color=vec4(data);}",
        );
        let id = call(&mut context, "getUniformLocation", &[owner as i64], "data")
            .as_u64()
            .unwrap() as u32;
        call(&mut context, "deleteProgram", &[owner as i64], "");
        assert!(context.objects.uniform_location(id).is_ok());
        assert_eq!(
            context.dispatch(
                &Command {
                    op: "uniform1f".into(),
                    i: vec![id as i64],
                    f: vec![0.25],
                    text: String::new()
                },
                None
            ),
            Ok(Value::Null)
        );
        assert_eq!(
            call(&mut context, "getUniform", &[owner as i64, id as i64], "")["values"],
            json!([0.25])
        );
        call(&mut context, "useProgram", &[0], "");
        assert!(context.objects.uniform_location(id).is_err());
        assert!(context.objects.get(owner, Kind::Program).is_err());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn gpu_object_admission_is_bounded_and_recovers_after_retirement() {
    session::run_native_test(|| {
        let mut context = version_two();
        let initial_storage = context.resource_bytes;
        let mut buffers = Vec::new();
        for _ in 0..MAX_OBJECTS {
            buffers.push(
                call(&mut context, "createBuffer", &[], "")
                    .as_u64()
                    .unwrap() as u32,
            );
        }
        assert_eq!(
            context.resource_bytes, initial_storage,
            "empty names reserve no buffer storage"
        );
        let creation = Command {
            op: "createBuffer".into(),
            i: vec![],
            f: vec![],
            text: String::new(),
        };
        assert_eq!(context.dispatch(&creation, None), Err(gl::OUT_OF_MEMORY));
        let retired = buffers[0];
        call(&mut context, "deleteBuffer", &[retired as i64], "");
        let replacement = call(&mut context, "createBuffer", &[], "")
            .as_u64()
            .unwrap() as u32;
        assert_ne!(replacement, retired);
        assert!(context.objects.get(retired, Kind::Buffer).is_err());
        assert!(context.objects.get(replacement, Kind::Buffer).is_ok());
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        // Admission still protects driver/browser bookkeeping after recovery.
        assert_eq!(context.dispatch(&creation, None), Err(gl::OUT_OF_MEMORY));
    });
}
