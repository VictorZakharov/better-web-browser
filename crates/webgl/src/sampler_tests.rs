//! Samplers change GPU filtering without mutating the texture object's state.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

fn sampler(context: &mut WebGl) -> u32 {
    call(context, "createSampler", &[], "").as_u64().unwrap() as u32
}
fn query(context: &mut WebGl, id: u32, pname: u32) -> Value {
    call(
        context,
        "getSamplerParameter",
        &[id as i64, pname as i64],
        "",
    )
}
fn sample(context: &mut WebGl) -> Vec<u8> {
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(0));
    context.surface.snapshot().unwrap()
}

#[test]
fn webgl2_sampler_wrap_overrides_texture_state_and_deletion_restores_it() {
    session::run_native_test(|| {
        let mut context = version_two();
        let texture = call(&mut context, "createTexture", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindTexture",
            &[gl::TEXTURE_2D as i64, texture as i64],
            "",
        );
        for (pname, value) in [
            (gl::TEXTURE_MIN_FILTER, gl::NEAREST),
            (gl::TEXTURE_MAG_FILTER, gl::NEAREST),
            (gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE),
        ] {
            call(
                &mut context,
                "texParameteri",
                &[gl::TEXTURE_2D as i64, pname as i64, value as i64],
                "",
            );
        }
        let upload = Command {
            op: "texImage2D".into(),
            i: vec![
                gl::TEXTURE_2D as i64,
                0,
                0x8058,
                2,
                1,
                0,
                gl::RGBA as i64,
                gl::UNSIGNED_BYTE as i64,
            ],
            f: vec![],
            text: String::new(),
        };
        context
            .dispatch(&upload, Some(&[255, 0, 0, 255, 0, 255, 0, 255]))
            .unwrap();
        program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision highp float;uniform sampler2D image;out vec4 color;void main(){color=texture(image,vec2(1.25,0.5));}",
        );
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
        let id = sampler(&mut context);
        for pname in [gl::TEXTURE_MIN_FILTER, gl::TEXTURE_MAG_FILTER] {
            call(
                &mut context,
                "samplerParameteri",
                &[id as i64, pname as i64, gl::NEAREST as i64],
                "",
            );
        }
        call(&mut context, "bindSampler", &[0, id as i64], "");
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        assert_eq!(
            call(
                &mut context,
                "getTexParameter",
                &[gl::TEXTURE_2D as i64, gl::TEXTURE_WRAP_S as i64],
                ""
            ),
            json!(gl::CLAMP_TO_EDGE)
        );
        call(&mut context, "deleteSampler", &[id as i64], "");
        assert_eq!(
            call(&mut context, "getParameter", &[0x8919], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "isSampler", &[id as i64], ""),
            json!(false)
        );
        assert!(
            sample(&mut context)
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
    });
}

#[test]
fn webgl2_sampler_bindings_are_unit_local_and_deletion_unbinds_every_reference() {
    session::run_native_test(|| {
        let mut context = version_two();
        let shared = sampler(&mut context);
        let other = sampler(&mut context);
        for unit in [0, 1, 2] {
            call(&mut context, "bindSampler", &[unit, shared as i64], "");
        }
        call(&mut context, "bindSampler", &[3, other as i64], "");
        for unit in [0, 1, 2, 3] {
            call(
                &mut context,
                "activeTexture",
                &[gl::TEXTURE0 as i64 + unit],
                "",
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x8919], ""),
                json!(if unit == 3 { other } else { shared })
            );
        }
        call(&mut context, "deleteSampler", &[shared as i64], "");
        for unit in [0, 1, 2] {
            call(
                &mut context,
                "activeTexture",
                &[gl::TEXTURE0 as i64 + unit],
                "",
            );
            assert_eq!(
                call(&mut context, "getParameter", &[0x8919], ""),
                Value::Null
            );
        }
        call(
            &mut context,
            "activeTexture",
            &[gl::TEXTURE0 as i64 + 3],
            "",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8919], ""),
            json!(other)
        );
    });
}

#[test]
fn webgl2_sampler_queries_use_exact_scalar_types_and_invalid_parameters_are_atomic() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = sampler(&mut context);
        assert_eq!(
            query(&mut context, id, gl::TEXTURE_WRAP_S),
            json!(gl::REPEAT)
        );
        let command = Command {
            op: "samplerParameterf".into(),
            i: vec![id as i64, 0x813a],
            f: vec![-2.5],
            text: String::new(),
        };
        context.dispatch(&command, None).unwrap();
        assert_eq!(query(&mut context, id, 0x813a), json!(-2.5));
        for (op, args, error) in [
            (
                "samplerParameteri",
                vec![id as i64, gl::TEXTURE_WRAP_S as i64, 0xdead],
                gl::INVALID_ENUM,
            ),
            (
                "getSamplerParameter",
                vec![id as i64, 0x813c],
                gl::INVALID_ENUM,
            ),
            (
                "bindSampler",
                vec![context.samplers.len() as i64, id as i64],
                gl::INVALID_VALUE,
            ),
        ] {
            assert_eq!(
                context.dispatch(
                    &Command {
                        op: op.into(),
                        i: args,
                        f: vec![],
                        text: String::new()
                    },
                    None
                ),
                Err(error)
            );
        }
        // Drain the native invalid-enum set before checking a later valid query.
        call(&mut context, "getError", &[], "");
        assert_eq!(
            query(&mut context, id, gl::TEXTURE_WRAP_S),
            json!(gl::REPEAT)
        );
        let mut peer = version_two();
        assert_eq!(call(&mut peer, "isSampler", &[id as i64], ""), json!(false));
        assert_eq!(
            peer.dispatch(
                &Command {
                    op: "bindSampler".into(),
                    i: vec![0, id as i64],
                    f: vec![],
                    text: String::new()
                },
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
        let mut one = WebGl::new(4, 4, Options::default()).unwrap();
        assert_eq!(
            one.dispatch(
                &Command {
                    op: "createSampler".into(),
                    i: vec![],
                    f: vec![],
                    text: String::new()
                },
                None
            ),
            Err(gl::INVALID_OPERATION)
        );
    });
}
