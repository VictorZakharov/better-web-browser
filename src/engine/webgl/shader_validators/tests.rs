use super::*;
use crate::engine::webgl::{Options, WebGl, session, shader_validator_resources};

fn environment(api: ApiVersion, kind: u32) -> Environment {
    Environment {
        api,
        kind,
        derivatives: false,
        frag_depth: false,
        texture_lod: false,
        draw_buffers: false,
        multi_draw: false,
        max_draw_buffers: 1,
    }
}

fn fresh(
    environment: Environment,
    resources: &BuiltInResources,
    source: &str,
) -> Result<String, String> {
    let validator = if environment.api == ApiVersion::Two {
        ShaderValidator::for_webgl2(environment.kind, Output::Essl, resources)
    } else {
        ShaderValidator::for_webgl(environment.kind, Output::Essl, resources)
    }
    .unwrap();
    let mut options = shaders::CompileOptions::mozangle();
    options.set_emulateGLDrawID(u64::from(environment.multi_draw));
    validator
        .compile(&[source], options)
        .map_err(|_| validator.info_log())?;
    Ok(validator.object_code())
}

#[test]
fn shader_validators_reuse_builtins_without_reusing_previous_author_symbols() {
    session::run_native_test(|| {
        let context = WebGl::new(4, 4, Options::default()).unwrap();
        let resources = shader_validator_resources::current(&context);
        let environment = environment(ApiVersion::One, gl::VERTEX_SHADER);
        let mut validators = Validators::default();
        validators.prepare(environment, &resources).unwrap();
        for index in 0..MAX_USES {
            let source = format!(
                "attribute vec4 position{index}; uniform mat4 transform{index};\n\
                 void main(){{gl_Position=transform{index}*position{index};}}"
            );
            assert_eq!(
                validators.compile(environment, &source),
                fresh(environment, &resources, &source),
                "fresh and reused actual ANGLE output must agree"
            );
            assert_eq!(validators.constructions, 1);
        }
        assert!(validators.needs_resources(environment).unwrap());
        assert!(validators.stages.iter().all(Option::is_none));
        validators.prepare(environment, &resources).unwrap();
        assert_eq!(validators.constructions, 2);
    });
}

#[test]
fn shader_validators_have_independent_bounded_vertex_and_fragment_slots() {
    session::run_native_test(|| {
        let context = WebGl::new(4, 4, Options::default()).unwrap();
        let resources = shader_validator_resources::current(&context);
        let vertex = environment(ApiVersion::One, gl::VERTEX_SHADER);
        let fragment = environment(ApiVersion::One, gl::FRAGMENT_SHADER);
        let mut validators = Validators::default();
        for (environment, source) in [
            (vertex, "void main(){gl_Position=vec4(1.);}"),
            (
                fragment,
                "precision mediump float;void main(){gl_FragColor=vec4(.5);}",
            ),
        ] {
            validators.prepare(environment, &resources).unwrap();
            assert_eq!(
                validators.compile(environment, source),
                fresh(environment, &resources, source)
            );
        }
        assert_eq!(validators.constructions, 2);
        assert_eq!(validators.stages.iter().flatten().count(), 2);
        assert!(!validators.needs_resources(vertex).unwrap());
        assert!(!validators.needs_resources(fragment).unwrap());
        assert_eq!(validators.stages[0].as_ref().unwrap().uses, 1);
        assert_eq!(validators.stages[1].as_ref().unwrap().uses, 1);
        // Invalid internal routing cannot alias a stage or reuse old output.
        let mut invalid = vertex;
        invalid.kind = 0;
        assert!(validators.needs_resources(invalid).is_err());
        assert!(validators.prepare(invalid, &resources).is_err());
        assert!(validators.compile(invalid, "void main(){}").is_err());
    });
}

#[test]
fn shader_validators_invalid_sources_and_failure_logs_never_poison_a_later_compile() {
    session::run_native_test(|| {
        let context = WebGl::new(4, 4, Options::default()).unwrap();
        let resources = shader_validator_resources::current(&context);
        let environment = environment(ApiVersion::One, gl::VERTEX_SHADER);
        let mut validators = Validators::default();
        for source in [
            "void main(){gl_Position=vec4(1.);}",
            "void main(){gl_Position=notDeclared;}",
            "void main(){gl_Position=vec4(.25);}",
            "void main(){gl_Position=vec4(1.);}\0",
            "void main(){gl_Position=vec4(.75);}",
        ] {
            if validators.needs_resources(environment).unwrap() {
                validators.prepare(environment, &resources).unwrap();
            }
            let actual = validators.compile(environment, source);
            let expected = fresh(environment, &resources, source);
            assert_eq!(actual.is_ok(), expected.is_ok());
            if let Ok(output) = actual {
                assert_eq!(output, expected.unwrap());
            } else {
                assert!(validators.needs_resources(environment).unwrap());
            }
        }
        assert_eq!(validators.constructions, 3);
    });
}

#[test]
fn shader_validators_environment_changes_replace_handles_not_validation_rules() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        let base = environment(ApiVersion::One, gl::FRAGMENT_SHADER);
        let mut enabled = base;
        enabled.derivatives = true;
        let mut validators = Validators::default();
        let resources = shader_validator_resources::current(&context);
        validators.prepare(base, &resources).unwrap();
        let ordinary = "precision mediump float;void main(){gl_FragColor=vec4(.5);}";
        validators.compile(base, ordinary).unwrap();
        assert!(validators.needs_resources(enabled).unwrap());
        // A mismatched handle must fail closed rather than silently using the
        // wrong resource environment. Only the owner may install its limits.
        assert!(validators.compile(enabled, ordinary).is_err());
        context.extensions.derivatives = true;
        let enabled_resources = shader_validator_resources::current(&context);
        validators.prepare(enabled, &enabled_resources).unwrap();
        let derivative = "#extension GL_OES_standard_derivatives : require\n\
            precision mediump float;void main(){gl_FragColor=vec4(dFdx(.5));}";
        assert_eq!(
            validators.compile(enabled, derivative),
            fresh(enabled, &enabled_resources, derivative)
        );
        assert!(validators.needs_resources(base).unwrap());
        validators.prepare(base, &resources).unwrap();
        assert_eq!(
            validators.compile(base, derivative).is_ok(),
            fresh(base, &resources, derivative).is_ok()
        );
        assert!(
            validators.needs_resources(base).unwrap(),
            "failed compile must retire its handle"
        );
        assert_eq!(validators.constructions, 3);
    });
}

#[test]
fn shader_validators_large_success_is_returned_but_not_retained() {
    session::run_native_test(|| {
        let context = WebGl::new(4, 4, Options::default()).unwrap();
        let resources = shader_validator_resources::current(&context);
        let environment = environment(ApiVersion::One, gl::VERTEX_SHADER);
        let mut validators = Validators::default();
        validators.prepare(environment, &resources).unwrap();
        let source = format!(
            "/*{}*/void main(){{gl_Position=vec4(1.);}}",
            " ".repeat(RETAIN_SOURCE_BYTES)
        );
        assert_eq!(
            validators.compile(environment, &source),
            fresh(environment, &resources, &source)
        );
        assert!(validators.needs_resources(environment).unwrap());
        assert!(validators.stages.iter().all(Option::is_none));
    });
}

#[test]
fn shader_validators_context_restore_and_webgl_versions_have_separate_owners() {
    session::run_native_test(|| {
        for api in [ApiVersion::One, ApiVersion::Two] {
            let mut first = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Options::default()
                },
            )
            .unwrap();
            let source = if api == ApiVersion::Two {
                "#version 300 es\nvoid main(){gl_Position=vec4(1.);}"
            } else {
                "void main(){gl_Position=vec4(1.);}"
            };
            let original = first.translate_shader(gl::VERTEX_SHADER, source).unwrap();
            assert_eq!(first.shader_validators.constructions, 1);
            let mut peer = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_eq!(peer.shader_validators.constructions, 0);
            assert_eq!(
                peer.translate_shader(gl::VERTEX_SHADER, source).unwrap(),
                original
            );
            assert_eq!(peer.shader_validators.constructions, 1);
            drop(first);
            let mut restored = WebGl::new(
                4,
                4,
                Options {
                    api,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_eq!(restored.shader_validators.constructions, 0);
            assert_eq!(
                restored
                    .translate_shader(gl::VERTEX_SHADER, source)
                    .unwrap(),
                original
            );
        }
    });
}

#[test]
fn retained_validators_drop_before_the_last_native_compiler_and_restore_cleanly() {
    session::run_native_test(|| {
        use crate::engine::webgl::api_version_tests::{call, compile, link};
        for api in [ApiVersion::One, ApiVersion::Two] {
            for query_success in [false, true] {
                // No peer context keeps ANGLE's global compiler TLS alive here.
                // Test both outstanding native work and already completed work.
                for _ in 0..3 {
                    let mut context = WebGl::new(
                        4,
                        4,
                        Options {
                            api,
                            ..Options::default()
                        },
                    )
                    .unwrap();
                    let vertex_source = if api == ApiVersion::Two {
                        "#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}"
                    } else {
                        "void main(){gl_Position=vec4(0,0,0,1);}"
                    };
                    let fragment_source = if api == ApiVersion::Two {
                        "#version 300 es\nprecision mediump float;out vec4 color;void main(){color=vec4(0,1,0,1);}"
                    } else {
                        "precision mediump float;void main(){gl_FragColor=vec4(0,1,0,1);}"
                    };
                    let vertex = compile(&mut context, gl::VERTEX_SHADER, vertex_source);
                    let fragment = compile(&mut context, gl::FRAGMENT_SHADER, fragment_source);
                    let owner = link(&mut context, vertex, fragment);
                    assert_eq!(context.shader_validators.stages.iter().flatten().count(), 2);
                    if query_success {
                        assert_eq!(
                            call(
                                &mut context,
                                "getProgramParameter",
                                &[owner as i64, gl::LINK_STATUS as i64],
                                ""
                            ),
                            serde_json::json!(true)
                        );
                    }
                    drop(context);
                    // Previously this drop crashed in the validator pool allocator,
                    // after native compiler destruction had finalized its TLS.
                }
            }
        }
    });
}
