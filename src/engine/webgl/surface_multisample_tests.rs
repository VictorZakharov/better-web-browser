//! Default antialiasing is measured from real native samples and edge pixels.
use super::api_version_tests::call;
use super::core_uniform_tests::program;
use super::*;

fn context(antialias: bool) -> WebGl {
    WebGl::new(
        8,
        8,
        Options {
            api: ApiVersion::Two,
            antialias,
            ..Options::default()
        },
    )
    .unwrap()
}

fn read(context: &mut WebGl) -> Result<Vec<u8>> {
    context.read_pixels(
        &Command {
            op: "readPixels".into(),
            i: vec![0, 0, 8, 8, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 256],
            f: vec![],
            text: String::new(),
        },
        None,
    )
}

fn triangle(context: &mut WebGl) {
    super::volume_copy_tests::clear(context, [0.0, 0.0, 0.0, 0.0]);
    program(
        context,
        "#version 300 es\nvoid main(){vec2 p[3]=vec2[3](vec2(-1,-1),vec2(1,-1),vec2(-1,1));gl_Position=vec4(p[gl_VertexID],0,1);}",
        "#version 300 es\nprecision highp float;out vec4 color;void main(){color=vec4(1);}",
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
}

#[test]
fn webgl2_default_antialias_has_real_four_sample_edge_coverage() {
    session::run_native_test(|| {
        for antialias in [false, true] {
            let mut context = context(antialias);
            assert_eq!(
                call(&mut context, "getParameter", &[gl::SAMPLES as i64], ""),
                json!(if antialias { 4 } else { 0 })
            );
            assert_eq!(context.resource_bytes, 64 * if antialias { 36 } else { 8 });
            triangle(&mut context);
            let pixels = context.surface.snapshot().unwrap();
            let partial = pixels
                .chunks_exact(4)
                .filter(|p| p[0] > 0 && p[0] < 255)
                .count();
            if antialias {
                assert!(partial > 0, "no partial edge pixels: {pixels:?}");
            } else {
                assert_eq!(partial, 0);
            }
            assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
        }
    });
}

#[test]
fn webgl2_default_multisample_storage_matches_depth_stencil_and_alpha_attributes() {
    session::run_native_test(|| {
        for (alpha, depth, stencil) in [
            (true, false, false),
            (false, false, false),
            (true, true, false),
            (true, false, true),
            (false, true, true),
        ] {
            let mut context = WebGl::new(
                4,
                4,
                Options {
                    api: ApiVersion::Two,
                    antialias: true,
                    alpha,
                    depth,
                    stencil,
                    ..Options::default()
                },
            )
            .unwrap();
            assert_eq!(
                call(
                    &mut context,
                    "checkFramebufferStatus",
                    &[gl::FRAMEBUFFER as i64],
                    ""
                ),
                json!(gl::FRAMEBUFFER_COMPLETE)
            );
            assert_eq!(
                call(&mut context, "getParameter", &[gl::SAMPLES as i64], ""),
                json!(4)
            );
            let pixels = context.surface.snapshot().unwrap();
            assert!(
                pixels
                    .chunks_exact(4)
                    .all(|p| p == [0, 0, 0, if alpha { 0 } else { 255 }])
            );
            assert_eq!(
                context.resource_bytes,
                16 * if depth || stencil { 36 } else { 20 }
            );
        }
    });
}

#[test]
fn webgl2_default_multisample_readback_resolves_without_rebinding_author_framebuffers() {
    session::run_native_test(|| {
        let mut context = context(true);
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let draw = super::array_copy_boundary_tests::framebuffer(&mut context, 0x8058);
        super::volume_copy_tests::clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        call(
            &mut context,
            "bindFramebuffer",
            &[super::framebuffer_guard::READ as i64, 0],
            "",
        );
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[0, 0, 0, 0], "");
        call(&mut context, "colorMask", &[0, 0, 0, 0], "");
        let pixels = read(&mut context).unwrap();
        assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(draw)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            Value::Null
        );
        assert_eq!(
            call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
            json!(true)
        );
        let mut native = 0;
        unsafe {
            gl::GetIntegerv(0x8caa, &mut native);
        }
        assert_eq!(native as u32, context.surface.framebuffer);
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        assert_eq!(read(&mut context), Err(gl::INVALID_OPERATION));
        let pixels = context.surface.snapshot().unwrap();
        assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 0, 255]));
        assert_eq!(
            call(&mut context, "getParameter", &[0x0c02], ""),
            json!(gl::NONE)
        );
        unsafe {
            gl::GetIntegerv(0x0c02, &mut native);
        }
        assert_eq!(native as u32, gl::NONE);
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::COLOR_WRITEMASK as i64],
                ""
            ),
            json!([false, false, false, false])
        );
    });
}

#[test]
fn webgl2_default_multisample_pbo_reads_retain_actual_resolved_pixels() {
    session::run_native_test(|| {
        let mut context = context(true);
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 1.0, 1.0]);
        let buffer = call(&mut context, "createBuffer", &[], "")
            .as_u64()
            .unwrap();
        call(&mut context, "bindBuffer", &[0x88eb, buffer as i64], "");
        call(
            &mut context,
            "bufferData",
            &[0x88eb, 260, gl::STATIC_DRAW as i64],
            "",
        );
        call(
            &mut context,
            "readPixelsToBuffer",
            &[0, 0, 8, 8, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 4],
            "",
        );
        let pixels = context
            .read_buffer(&Command {
                op: "getBufferSubData".into(),
                i: vec![0x88eb, 4, 256],
                f: vec![],
                text: String::new(),
            })
            .unwrap();
        assert!(pixels.chunks_exact(4).all(|p| p == [255, 0, 255, 255]));
        assert_eq!(
            call(&mut context, "getParameter", &[0x88ed], ""),
            json!(buffer)
        );
        let capture = context.surface.snapshot().unwrap();
        assert_eq!(pixels, capture);
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn webgl2_default_multisample_texture_copies_resolve_only_the_private_default_buffer() {
    session::run_native_test(|| {
        let mut context = context(true);
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let target = super::texture_targets::VOLUME;
        super::compressed_texture_tests::texture(&mut context, target);
        call(
            &mut context,
            "texStorage3D",
            &[target as i64, 1, 0x8058, 8, 8, 2],
            "",
        );
        call(
            &mut context,
            "copyTexSubImage3D",
            &[target as i64, 0, 0, 0, 1, 0, 0, 8, 8],
            "",
        );
        program(
            &mut context,
            super::core_uniform_tests::VERTEX,
            "#version 300 es\nprecision highp float;uniform highp sampler3D t;out vec4 color;void main(){bool valid=all(equal(textureLod(t,vec3(0.5,0.5,0.75),0.0),vec4(1,0,0,1)))&&all(equal(textureLod(t,vec3(0.5,0.5,0.25),0.0),vec4(0)));color=valid?vec4(0,1,0,1):vec4(1,0,0,1);}",
        );
        call(
            &mut context,
            "drawArrays",
            &[gl::TRIANGLES as i64, 0, 3],
            "",
        );
        assert!(
            read(&mut context)
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [0, 255, 0, 255])
        );
    });
}

#[test]
fn webgl2_default_multisample_resize_reclaims_storage_and_preserves_failed_resize_pixels() {
    session::run_native_test(|| {
        let mut context = context(true);
        super::volume_copy_tests::clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let mut command = Command {
            op: "resize".into(),
            i: vec![16, 8],
            f: vec![],
            text: String::new(),
        };
        let budget = context.resource_limit;
        let bytes = context.resource_bytes;
        context.resource_limit = bytes;
        assert_eq!(context.dispatch(&command, None), Err(gl::OUT_OF_MEMORY));
        assert_eq!(context.resource_bytes, bytes);
        assert!(
            read(&mut context)
                .unwrap()
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        context.resource_limit = budget;
        context.dispatch(&command, None).unwrap();
        assert_eq!(context.resource_bytes, 128 * 36);
        assert_eq!(
            call(&mut context, "getParameter", &[gl::SAMPLES as i64], ""),
            json!(4)
        );
        assert!(context.surface.snapshot().unwrap().iter().all(|v| *v == 0));
        command.i = vec![8, 8];
        context.dispatch(&command, None).unwrap();
        assert_eq!(context.resource_bytes, bytes);
        super::volume_copy_tests::clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        context.dispatch(&command, None).unwrap();
        assert_eq!(context.resource_bytes, bytes);
        assert!(read(&mut context).unwrap().iter().all(|v| *v == 0));
    });
}
