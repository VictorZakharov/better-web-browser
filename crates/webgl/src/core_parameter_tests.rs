//! Native capabilities have closed reply widths and version admission.
use super::api_version_tests::{call, version_two};
use super::core_uniform_tests::{VERTEX, program};
use super::*;

#[test]
fn webgl2_core_limits_are_native_scalars_and_not_webgl1_capabilities() {
    session::run_native_test(|| {
        let mut context = version_two();
        for pname in [
            0x8073, 0x88ff, 0x80e8, 0x80e9, 0x8b4a, 0x8b49, 0x8b4b, 0x8a2b, 0x8a2d, 0x8a2e, 0x8a2f,
            0x8a34, 0x8904, 0x8905, 0x8c80, 0x8c8a, 0x8c8b, 0x8d57, 0x9122, 0x9125, 0x8a30, 0x8a31,
            0x8a33, 0x8d6b, 0x9111,
        ] {
            let value = call(&mut context, "getParameter", &[pname], "");
            assert!(value.is_i64() || value.is_u64(), "{pname:x}: {value}");
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
        assert!(
            call(&mut context, "getParameter", &[0x8d6b], "")
                .as_u64()
                .unwrap()
                >= 0xffffff
        );
        drop(context);
        let mut one = WebGl::new(2, 2, Options::default()).unwrap();
        assert_eq!(
            one.dispatch(
                &Command {
                    op: "getParameter".into(),
                    i: vec![0x8d6b],
                    f: vec![],
                    text: String::new()
                },
                None
            ),
            Err(gl::INVALID_ENUM)
        );
    });
}

#[test]
fn webgl2_rasterizer_discard_prevents_pixels_without_losing_draw_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        program(
            &mut context,
            VERTEX,
            "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(1,0,0,1);}",
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8c89], ""),
            json!(false)
        );
        call(&mut context, "enable", &[0x8c89], "");
        assert_eq!(call(&mut context, "isEnabled", &[0x8c89], ""), json!(true));
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
                .iter()
                .all(|byte| *byte == 0)
        );
        call(&mut context, "disable", &[0x8c89], "");
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
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
