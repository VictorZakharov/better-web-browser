//! Native allocation churn must not consume a lifetime-total budget. Deleted
//! objects retained by programs, VAOs and FBOs still count until final release.
use super::*;
use crate::engine::webgl::{
    api_version_tests::{call, compile, link, version_two},
    compressed_texture_tests::upload,
    session,
    transform_delete_tests::retained_array,
    transform_feedback_tests::buffer,
};

fn renderbuffer(context: &mut WebGl) -> i64 {
    let id = call(context, "createRenderbuffer", &[], "")
        .as_i64()
        .unwrap();
    call(
        context,
        "bindRenderbuffer",
        &[gl::RENDERBUFFER as i64, id],
        "",
    );
    id
}

#[test]
fn resource_budget_buffer_churn_reclaims_only_final_retirement() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        context.resource_limit = baseline + 64;
        for _ in 0..128 {
            let id = buffer(&mut context, 16);
            assert_eq!(context.resource_bytes, baseline + 32);
            call(&mut context, "deleteBuffer", &[id], "");
            assert_eq!(context.resource_bytes, baseline);
        }
        let id = buffer(&mut context, 16);
        let vao = retained_array(&mut context, id);
        call(&mut context, "bindVertexArray", &[0], "");
        call(&mut context, "deleteBuffer", &[id], "");
        assert_eq!(context.resource_bytes, baseline + 32);
        call(&mut context, "deleteBuffer", &[id], "");
        assert_eq!(context.resource_bytes, baseline + 32);
        call(&mut context, "deleteVertexArray", &[vao], "");
        assert_eq!(context.resource_bytes, baseline);
        context.reclaim_retired_storage().unwrap();
        assert_eq!(context.resource_bytes, baseline);
    });
}

#[test]
fn resource_budget_renderbuffer_redefinition_is_high_water_not_cumulative() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        let id = renderbuffer(&mut context);
        for _ in 0..128 {
            call(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, 0x8058, 8, 8],
                "",
            );
        }
        assert_eq!(context.resource_bytes, baseline + 8 * 8 * 4 * 2);
        for size in [4, 2, 8, 1] {
            call(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, 0x8058, size, size],
                "",
            );
            assert_eq!(context.resource_bytes, baseline + 8 * 8 * 4 * 2);
        }
        call(
            &mut context,
            "renderbufferStorage",
            &[gl::RENDERBUFFER as i64, 0x8058, 16, 16],
            "",
        );
        assert_eq!(context.resource_bytes, baseline + 16 * 16 * 4 * 2);
        call(&mut context, "deleteRenderbuffer", &[id], "");
        assert_eq!(context.resource_bytes, baseline);
    });
}

#[test]
fn resource_budget_multisample_redefinitions_share_single_object_reservation() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        let id = renderbuffer(&mut context);
        for _ in 0..64 {
            call(
                &mut context,
                "renderbufferStorageMultisample",
                &[gl::RENDERBUFFER as i64, 2, 0x8058, 8, 8],
                "",
            );
        }
        let charge = context.resource_bytes;
        assert!(charge >= baseline + 8 * 8 * 4 * 2 * 2);
        call(
            &mut context,
            "renderbufferStorage",
            &[gl::RENDERBUFFER as i64, 0x8058, 4, 4],
            "",
        );
        assert_eq!(context.resource_bytes, charge);
        call(&mut context, "deleteRenderbuffer", &[id], "");
        assert_eq!(context.resource_bytes, baseline);
    });
}

#[test]
fn resource_budget_deleted_renderbuffer_waits_for_inactive_framebuffer() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        let id = renderbuffer(&mut context);
        call(
            &mut context,
            "renderbufferStorage",
            &[gl::RENDERBUFFER as i64, 0x8058, 8, 8],
            "",
        );
        let framebuffer = call(&mut context, "createFramebuffer", &[], "")
            .as_i64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, framebuffer],
            "",
        );
        call(
            &mut context,
            "framebufferRenderbuffer",
            &[
                gl::FRAMEBUFFER as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::RENDERBUFFER as i64,
                id,
            ],
            "",
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, 0],
            "",
        );
        call(&mut context, "deleteRenderbuffer", &[id], "");
        assert_eq!(context.resource_bytes, baseline + 8 * 8 * 4 * 2);
        call(&mut context, "deleteFramebuffer", &[framebuffer], "");
        assert_eq!(context.resource_bytes, baseline);
    });
}

#[test]
fn resource_budget_shader_charge_waits_for_deleted_current_program() {
    session::run_native_test(|| {
        let mut context = version_two();
        let baseline = context.resource_bytes;
        let vertex = compile(
            &mut context,
            gl::VERTEX_SHADER,
            "#version 300 es\nvoid main(){gl_Position=vec4(0,0,0,1);}",
        );
        let fragment = compile(
            &mut context,
            gl::FRAGMENT_SHADER,
            "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}",
        );
        let program = link(&mut context, vertex, fragment);
        call(&mut context, "useProgram", &[program as i64], "");
        let charged = context.resource_bytes;
        for shader in [vertex, fragment] {
            call(&mut context, "deleteShader", &[shader as i64], "");
        }
        call(&mut context, "deleteProgram", &[program as i64], "");
        assert_eq!(context.resource_bytes, charged);
        call(&mut context, "useProgram", &[0], "");
        assert_eq!(context.resource_bytes, baseline);
    });
}

#[test]
fn resource_budget_rejected_redefinition_preserves_counter_capacity_and_native_size() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = renderbuffer(&mut context);
        call(
            &mut context,
            "renderbufferStorage",
            &[gl::RENDERBUFFER as i64, 0x8058, 4, 4],
            "",
        );
        let charged = context.resource_bytes;
        context.resource_limit = charged;
        assert_eq!(
            upload(
                &mut context,
                "renderbufferStorage",
                &[gl::RENDERBUFFER as i64, 0x8058, 8, 8],
                None
            ),
            Err(gl::OUT_OF_MEMORY)
        );
        assert_eq!(context.resource_bytes, charged);
        assert_eq!(
            context
                .objects
                .get(id as u32, Kind::Renderbuffer)
                .unwrap()
                .capacity,
            64
        );
        assert_eq!(
            call(
                &mut context,
                "getRenderbufferParameter",
                &[gl::RENDERBUFFER as i64, gl::RENDERBUFFER_WIDTH as i64],
                ""
            ),
            serde_json::json!(4)
        );
        // Preparing or abandoning a reservation never grants or consumes bytes.
        let _abandoned = context
            .prepare_object_storage(id as u32, Kind::Renderbuffer, 64)
            .unwrap();
        assert_eq!(context.resource_bytes, charged);
        assert!(
            context
                .prepare_object_storage(id as u32, Kind::Renderbuffer, usize::MAX)
                .is_err()
        );
        call(&mut context, "deleteRenderbuffer", &[id], "");
        assert!(context.resource_bytes < charged);
    });
}
