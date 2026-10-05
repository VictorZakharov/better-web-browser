use crate::engine::webgl::api_version_tests::{call, compile, compiled, version_two};
use crate::engine::webgl::*;

#[test]
fn repeated_validation_does_not_reuse_native_status_or_shader_objects() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = "#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}";
        let first = compile(&mut context, gl::VERTEX_SHADER, source);
        assert!(compiled(&mut context, first));
        assert_eq!(context.shader_validation_cache.hits, 0);
        let second = compile(&mut context, gl::VERTEX_SHADER, source);
        assert_ne!(first, second);
        assert!(compiled(&mut context, second));
        assert_eq!(context.shader_validation_cache.hits, 1);
        call(
            &mut context,
            "shaderSource",
            &[second as i64],
            "invalid shader",
        );
        call(&mut context, "compileShader", &[second as i64], "");
        assert!(!compiled(&mut context, second));
        assert!(compiled(&mut context, first));
        assert_eq!(context.shader_validation_cache.hits, 1);
        call(&mut context, "shaderSource", &[second as i64], source);
        call(&mut context, "compileShader", &[second as i64], "");
        assert!(compiled(&mut context, second));
        assert_eq!(context.shader_validation_cache.hits, 2);
        assert_eq!(
            call(&mut context, "getShaderInfoLog", &[second as i64], ""),
            json!("")
        );
    });
}

#[test]
fn shader_type_and_compiler_extension_admission_do_not_share_validation() {
    session::run_native_test(|| {
        let mut context = WebGl::new(4, 4, Options::default()).unwrap();
        let source = "precision highp float;void main(){}";
        let vertex = compile(&mut context, gl::VERTEX_SHADER, source);
        assert!(compiled(&mut context, vertex));
        let fragment = compile(&mut context, gl::FRAGMENT_SHADER, source);
        assert!(compiled(&mut context, fragment));
        assert_eq!(context.shader_validation_cache.hits, 0);
        if context.extensions.available_derivatives {
            call(
                &mut context,
                "enableExtension",
                &[],
                "OES_standard_derivatives",
            );
            let vertex = compile(&mut context, gl::VERTEX_SHADER, source);
            assert!(compiled(&mut context, vertex));
            assert_eq!(context.shader_validation_cache.hits, 0);
            let source = "#extension GL_OES_standard_derivatives : require\nprecision highp float;void main(){gl_FragColor=vec4(dFdx(1.0));}";
            let fragment = compile(&mut context, gl::FRAGMENT_SHADER, source);
            assert!(compiled(&mut context, fragment));
            let fragment = compile(&mut context, gl::FRAGMENT_SHADER, source);
            assert!(compiled(&mut context, fragment));
            assert_eq!(context.shader_validation_cache.hits, 1);
        }
    });
}

#[test]
fn failed_validation_is_retryable_and_does_not_enter_success_cache() {
    session::run_native_test(|| {
        let mut context = version_two();
        for _ in 0..2 {
            let shader = compile(
                &mut context,
                gl::VERTEX_SHADER,
                "#version 310 es\nvoid main(){}",
            );
            assert!(!compiled(&mut context, shader));
            let log = call(&mut context, "getShaderInfoLog", &[shader as i64], "");
            assert!(!log.as_str().unwrap().is_empty());
        }
        assert_eq!(context.shader_validation_cache.hits, 0);
    });
}
