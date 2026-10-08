//! Link-time snapshots and API barriers, exercised against the real WARP backend.
use super::{
    api_version_tests::{call, compile, link, version_two},
    *,
};

const VERTEX: &str = "#version 300 es\nvoid main(){vec2 p=vec2((gl_VertexID<<1)&2,gl_VertexID&2);gl_Position=vec4(p*2.-1.,0,1);}";
const GREEN: &str =
    "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0,1,0,1);}";
const BLUE: &str =
    "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(0,0,1,1);}";

fn material(context: &mut WebGl) -> (u32, [u32; 2]) {
    let vertex = compile(context, gl::VERTEX_SHADER, VERTEX);
    let fragment = compile(context, gl::FRAGMENT_SHADER, GREEN);
    (link(context, vertex, fragment), [vertex, fragment])
}

fn draw(context: &mut WebGl, program: u32, expected: [u8; 4]) {
    call(context, "useProgram", &[program as i64], "");
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    assert!(
        context
            .surface
            .snapshot()
            .unwrap()
            .chunks_exact(4)
            .all(|p| p == expected)
    );
    assert_eq!(call(context, "getError", &[], ""), json!(gl::NO_ERROR));
}

#[test]
fn shader_recompilation_preserves_all_previous_program_snapshots() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (first, shaders) = material(&mut context);
        let second = link(&mut context, shaders[0], shaders[1]);
        assert_eq!(context.pending_links.len(), 2);
        call(&mut context, "shaderSource", &[shaders[1] as i64], BLUE);
        assert!(context.pending_links.is_empty());
        call(&mut context, "compileShader", &[shaders[1] as i64], "");
        let third = link(&mut context, shaders[0], shaders[1]);
        draw(&mut context, first, [0, 255, 0, 255]);
        draw(&mut context, second, [0, 255, 0, 255]);
        draw(&mut context, third, [0, 0, 255, 255]);
    });
}

#[test]
fn detaching_deleted_shader_does_not_change_preceding_link() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, shaders) = material(&mut context);
        call(&mut context, "deleteShader", &[shaders[1] as i64], "");
        assert!(context.pending_links.contains(program));
        call(
            &mut context,
            "detachShader",
            &[program as i64, shaders[1] as i64],
            "",
        );
        assert!(context.pending_links.is_empty());
        draw(&mut context, program, [0, 255, 0, 255]);
    });
}

#[test]
fn link_status_and_reflection_resolve_the_requested_program() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, _) = material(&mut context);
        assert!(context.pending_links.contains(program));
        assert_eq!(
            call(
                &mut context,
                "getProgramParameter",
                &[program as i64, gl::LINK_STATUS as i64],
                ""
            ),
            json!(true)
        );
        assert!(!context.pending_links.contains(program));
        assert_eq!(
            call(&mut context, "getProgramInfoLog", &[program as i64], ""),
            json!("")
        );
        draw(&mut context, program, [0, 255, 0, 255]);
    });
}

#[test]
fn program_deletion_submits_pending_work_before_releasing_shader_references() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, shaders) = material(&mut context);
        for shader in shaders {
            call(&mut context, "deleteShader", &[shader as i64], "");
        }
        call(&mut context, "deleteProgram", &[program as i64], "");
        assert!(context.pending_links.is_empty());
        assert_eq!(context.objects.storage_summary().1, 0);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn private_scheduling_does_not_admit_public_completion_queries() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, _) = material(&mut context);
        let result = context.dispatch(
            &Command {
                op: "getProgramParameter".into(),
                i: vec![program as i64, 0x91b1],
                f: vec![],
                text: String::new(),
            },
            None,
        );
        assert_eq!(result, Err(gl::INVALID_ENUM));
        assert!(context.pending_links.contains(program));
        draw(&mut context, program, [0, 255, 0, 255]);
    });
}

#[test]
fn current_program_relink_is_submitted_immediately() {
    session::run_native_test(|| {
        let mut context = version_two();
        let (program, shaders) = material(&mut context);
        draw(&mut context, program, [0, 255, 0, 255]);
        call(&mut context, "shaderSource", &[shaders[1] as i64], BLUE);
        call(&mut context, "compileShader", &[shaders[1] as i64], "");
        call(&mut context, "linkProgram", &[program as i64], "");
        assert!(context.pending_links.is_empty());
        draw(&mut context, program, [0, 0, 255, 255]);
    });
}

#[test]
fn unobserved_context_shutdown_discards_only_owner_side_submissions() {
    session::run_native_test(|| {
        let mut context = version_two();
        for _ in 0..16 {
            material(&mut context);
        }
        assert_eq!(context.pending_links.len(), 16);
        drop(context);
        let mut replacement = version_two();
        let (program, _) = material(&mut replacement);
        draw(&mut replacement, program, [0, 255, 0, 255]);
    });
}
