//! Explicit validators can outlive a peer's last lazy native GLES compiler.
use super::*;
use crate::engine::webgl::{Options, WebGl, api_version_tests, session};

fn vertex(api: ApiVersion, position: u8) -> String {
    let version = if api == ApiVersion::Two {
        "#version 300 es\n"
    } else {
        ""
    };
    format!("{version}void main(){{gl_Position=vec4({position},0,0,1);}}")
}

fn retire_native_peer(api: ApiVersion) {
    let mut peer = WebGl::new(
        4,
        4,
        Options {
            api,
            ..Default::default()
        },
    )
    .unwrap();
    let shader = api_version_tests::compile(&mut peer, gl::VERTEX_SHADER, &vertex(api, 0));
    assert_eq!(
        api_version_tests::call(
            &mut peer,
            "getShaderParameter",
            &[shader as i64, gl::COMPILE_STATUS as i64],
            ""
        ),
        serde_json::json!(true)
    );
    drop(peer);
}

#[test]
fn uncompiled_context_validator_survives_a_peers_last_native_compiler_retirement() {
    session::run_native_test(|| {
        for api in [ApiVersion::One, ApiVersion::Two] {
            let mut retained = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Default::default()
                },
            )
            .unwrap();
            // Translation constructs an explicit validator, but does not cause
            // Context::getCompiler to create a native GLES compiler/refcount.
            retained
                .translate_shader(gl::VERTEX_SHADER, &vertex(api, 0))
                .unwrap();
            assert_eq!(
                retained.shader_validators.stages.iter().flatten().count(),
                1
            );
            retire_native_peer(api);
            // The peer may have finalized ANGLE's process-wide pool TLS.
            // Reuse must initialize it before entering the retained validator.
            retained.native.make_current().unwrap();
            retained
                .translate_shader(gl::VERTEX_SHADER, &vertex(api, 1))
                .unwrap();
            retire_native_peer(api);
            // Destruction needs the same protection even without another use.
            drop(retained);
        }
    });
}
