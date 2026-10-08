//! Real native task ownership, independent of timing or synthetic pending flags.
use super::{
    api_version_tests::{call, compile, link, version_two},
    *,
};

const VERTEX: &str = "#version 300 es\nvoid main(){vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);gl_Position=vec4(p*2.-1.,0,1);}";

fn material(context: &mut WebGl, variant: usize) -> (u32, [u32; 2]) {
    let vertex = compile(context, gl::VERTEX_SHADER, VERTEX);
    let operations = (0..64)
        .map(|index| format!("v=sin(v*1.01+float({}.)*.001)+v*.7;", index + variant))
        .collect::<String>();
    let fragment = compile(
        context,
        gl::FRAGMENT_SHADER,
        &format!(
            "#version 300 es\nprecision highp float;uniform float seed;out vec4 color;void main(){{float v=seed;{operations}color=vec4(clamp(v,0.,1.),1.,0.,1.);}}"
        ),
    );
    (link(context, vertex, fragment), [vertex, fragment])
}

fn draw_and_check(context: &mut WebGl, program: u32) {
    assert_eq!(
        call(
            context,
            "getProgramParameter",
            &[program as i64, gl::LINK_STATUS as i64],
            ""
        ),
        json!(true)
    );
    call(context, "useProgram", &[program as i64], "");
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert_eq!(call(context, "getError", &[], ""), json!(gl::NO_ERROR));
    assert!(
        context
            .surface
            .snapshot()
            .unwrap()
            .chunks_exact(4)
            .all(|pixel| pixel[1..] == [255, 0, 255])
    );
}

#[test]
fn private_parallel_scheduling_retains_deleted_shaders_and_does_not_grant_completion_api() {
    session::run_native_test(|| {
        let mut context = version_two();
        let submitted = compiler_workers::native_submissions();
        let mut programs = Vec::new();
        for variant in 0..12 {
            let (program, shaders) = material(&mut context, variant);
            for shader in shaders {
                call(&mut context, "deleteShader", &[shader as i64], "");
            }
            programs.push(program);
        }
        assert!(compiler_workers::native_submissions() > submitted);
        assert!(!context.extensions.parallel_compile);
        assert_eq!(
            context.dispatch(
                &Command {
                    op: "getProgramParameter".into(),
                    i: vec![programs[0] as i64, 0x91b1],
                    f: vec![],
                    text: String::new(),
                },
                None
            ),
            Err(gl::INVALID_ENUM)
        );
        for program in programs {
            draw_and_check(&mut context, program);
            call(&mut context, "deleteProgram", &[program as i64], "");
        }
        call(&mut context, "useProgram", &[0], "");
        assert_eq!(context.objects.storage_summary().1, 0);
    });
}

#[test]
fn peer_display_initialization_and_pending_program_retirement_keep_surviving_context_real() {
    session::run_native_test(|| {
        let mut first = version_two();
        for variant in 0..8 {
            let (program, shaders) = material(&mut first, variant);
            for shader in shaders {
                call(&mut first, "deleteShader", &[shader as i64], "");
            }
            // Deletion must resolve/retain native work, never free task pointers prematurely.
            call(&mut first, "deleteProgram", &[program as i64], "");
        }
        let (pending, shaders) = material(&mut first, 99);
        let mut survivor = version_two();
        survivor.native.make_current().unwrap();
        let (program, _) = material(&mut survivor, 100);
        first.native.make_current().unwrap();
        for shader in shaders {
            call(&mut first, "deleteShader", &[shader as i64], "");
        }
        call(&mut first, "deleteProgram", &[pending as i64], "");
        drop(first);
        survivor.native.make_current().unwrap();
        draw_and_check(&mut survivor, program);
    });
}

#[test]
fn relinking_before_completion_finishes_native_work_and_replaces_the_executable() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, shaders) = material(&mut context, 3);
        call(
            &mut context,
            "detachShader",
            &[program as i64, shaders[1] as i64],
            "",
        );
        call(&mut context, "deleteShader", &[shaders[1] as i64], "");
        let replacement = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0,0,1,1);}",
        );
        call(
            &mut context,
            "attachShader",
            &[program as i64, replacement as i64],
            "",
        );
        call(&mut context, "linkProgram", &[program as i64], "");
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(true)
        );
        call(&mut context, "useProgram", &[program as i64], "");
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
                .all(|pixel| pixel == [0, 0, 255, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(gl::NO_ERROR));
    });
}
