//! Read selection and format safety for native array-layer framebuffer copies.
use super::api_version_tests::{call, version_two};
use super::compressed_capabilities::Family;
use super::compressed_texture_tests::{enable, texture, upload};
use super::core_uniform_tests::{VERTEX, program};
use super::framebuffer_guard::{DRAW, READ};
use super::texture_targets::ARRAY;
use super::*;

fn clear(context: &mut WebGl, color: [f64; 4]) {
    context
        .dispatch(
            &Command {
                op: "clearColor".into(),
                i: vec![],
                f: color.to_vec(),
                text: String::new(),
            },
            None,
        )
        .unwrap();
    call(context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
}

fn framebuffer(context: &mut WebGl, format: u32) -> u32 {
    let texture = texture(context, gl::TEXTURE_2D);
    call(
        context,
        "texStorage2D",
        &[gl::TEXTURE_2D as i64, 1, format as i64, 4, 4],
        "",
    );
    let id = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, id as i64],
        "",
    );
    call(
        context,
        "framebufferTexture2D",
        &[
            gl::FRAMEBUFFER as i64,
            gl::COLOR_ATTACHMENT0 as i64,
            gl::TEXTURE_2D as i64,
            texture as i64,
            0,
        ],
        "",
    );
    assert_eq!(
        call(
            context,
            "checkFramebufferStatus",
            &[gl::FRAMEBUFFER as i64],
            ""
        ),
        json!(gl::FRAMEBUFFER_COMPLETE)
    );
    id
}

fn destination(context: &mut WebGl) {
    texture(context, ARRAY);
    call(
        context,
        "texStorage3D",
        &[ARRAY as i64, 1, 0x8058, 4, 4, 2],
        "",
    );
}

fn copy(context: &mut WebGl, source: [i64; 4]) -> Result<Value> {
    upload(
        context,
        "copyTexSubImage3D",
        &[
            ARRAY as i64,
            0,
            0,
            0,
            1,
            source[0],
            source[1],
            source[2],
            source[3],
        ],
        None,
    )
}

fn sample(context: &mut WebGl, uv: &str) -> Vec<u8> {
    call(context, "bindFramebuffer", &[gl::FRAMEBUFFER as i64, 0], "");
    program(
        context,
        VERTEX,
        &format!(
            "#version 300 es\nprecision highp float;uniform highp sampler2DArray t;out vec4 color;void main(){{color=texture(t,{uv});}}"
        ),
    );
    call(context, "drawArrays", &[gl::TRIANGLES as i64, 0, 3], "");
    context.surface.snapshot().unwrap()
}

#[test]
fn array_copy_uses_read_framebuffer_without_rebinding_the_draw_framebuffer() {
    session::run_native_test(|| {
        let mut context = version_two();
        let red = framebuffer(&mut context, 0x8058);
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        let green = framebuffer(&mut context, 0x8058);
        clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        destination(&mut context);
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, red as i64],
            "",
        );
        call(
            &mut context,
            "bindFramebuffer",
            &[DRAW as i64, green as i64],
            "",
        );
        copy(&mut context, [0, 0, 4, 4]).unwrap();
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::FRAMEBUFFER_BINDING as i64],
                ""
            ),
            json!(green)
        );
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(red)
        );
        let pixels = sample(&mut context, "vec3(0.5,0.5,1)");
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}

#[test]
fn array_copy_rejects_none_read_route_and_incomplete_source_before_writing() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, 0x8058);
        destination(&mut context);
        call(&mut context, "readBuffer", &[gl::NONE as i64], "");
        assert_eq!(copy(&mut context, [0, 0, 4, 4]), Err(gl::INVALID_OPERATION));
        let empty = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, empty as i64],
            "",
        );
        assert_eq!(
            copy(&mut context, [0, 0, 4, 4]),
            Err(gl::INVALID_FRAMEBUFFER_OPERATION)
        );
        let pixels = sample(&mut context, "vec3(0.5,0.5,1)");
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 0]));
    });
}

#[test]
fn array_copy_clips_source_outside_read_buffer_without_clearing_destination_neighbors() {
    session::run_native_test(|| {
        let mut context = version_two();
        destination(&mut context);
        clear(&mut context, [1.0, 0.0, 0.0, 1.0]);
        copy(&mut context, [0, 0, 4, 4]).unwrap();
        clear(&mut context, [0.0, 1.0, 0.0, 1.0]);
        copy(&mut context, [-2, 0, 4, 4]).unwrap();
        for (uv, color) in [
            ("vec3(0.25,0.5,1)", [255, 0, 0, 255]),
            ("vec3(0.75,0.5,1)", [0, 255, 0, 255]),
            ("vec3(0.75,0.5,0)", [0, 0, 0, 0]),
        ] {
            let pixels = sample(&mut context, uv);
            assert!(
                pixels.chunks_exact(4).all(|pixel| pixel == color),
                "{uv}: {pixels:?}"
            );
        }
    });
}

#[test]
fn array_copy_rejects_integer_source_to_normalized_destination() {
    session::run_native_test(|| {
        let mut context = version_two();
        framebuffer(&mut context, 0x8d7c); // RGBA8UI
        destination(&mut context);
        assert_eq!(copy(&mut context, [0, 0, 4, 4]), Err(gl::INVALID_OPERATION));
        let pixels = sample(&mut context, "vec3(0.5,0.5,1)");
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 0]));
    });
}

#[test]
fn array_copy_cannot_replace_compressed_storage_or_copy_into_its_read_attachment() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        let compressed = texture(&mut context, ARRAY);
        call(
            &mut context,
            "texStorage3D",
            &[ARRAY as i64, 1, 0x83f1, 4, 4, 2],
            "",
        );
        let baseline = context.resource_bytes;
        assert_eq!(copy(&mut context, [0, 0, 4, 4]), Err(gl::INVALID_OPERATION));
        assert_eq!(context.resource_bytes, baseline);
        assert_eq!(
            context
                .objects
                .get(compressed, Kind::Texture)
                .unwrap()
                .core_images[&(ARRAY, 0)]
                .internal,
            0x83f1
        );
        let destination = texture(&mut context, ARRAY);
        call(
            &mut context,
            "texStorage3D",
            &[ARRAY as i64, 1, 0x8058, 4, 4, 2],
            "",
        );
        let source = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[gl::FRAMEBUFFER as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "framebufferTextureLayer",
            &[
                gl::FRAMEBUFFER as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                destination as i64,
                0,
                1,
            ],
            "",
        );
        assert_eq!(copy(&mut context, [0, 0, 4, 4]), Err(gl::INVALID_OPERATION));
    });
}

#[test]
fn array_copy_rejects_multisampled_read_storage_without_resolving_it_implicitly() {
    session::run_native_test(|| {
        let mut context = version_two();
        destination(&mut context);
        let renderbuffer = call(&mut context, "createRenderbuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindRenderbuffer",
            &[gl::RENDERBUFFER as i64, renderbuffer as i64],
            "",
        );
        call(
            &mut context,
            "renderbufferStorageMultisample",
            &[gl::RENDERBUFFER as i64, 4, 0x8058, 4, 4],
            "",
        );
        let source = call(&mut context, "createFramebuffer", &[], "")
            .as_u64()
            .unwrap();
        call(
            &mut context,
            "bindFramebuffer",
            &[READ as i64, source as i64],
            "",
        );
        call(
            &mut context,
            "framebufferRenderbuffer",
            &[
                READ as i64,
                gl::COLOR_ATTACHMENT0 as i64,
                gl::RENDERBUFFER as i64,
                renderbuffer as i64,
            ],
            "",
        );
        assert_eq!(
            call(&mut context, "checkFramebufferStatus", &[READ as i64], ""),
            json!(gl::FRAMEBUFFER_COMPLETE)
        );
        assert_eq!(copy(&mut context, [0, 0, 4, 4]), Err(gl::INVALID_OPERATION));
        assert_eq!(
            call(&mut context, "getParameter", &[0x8caa], ""),
            json!(source)
        );
        let pixels = sample(&mut context, "vec3(0.5,0.5,1)");
        assert!(pixels.chunks_exact(4).all(|pixel| pixel == [0, 0, 0, 0]));
    });
}

#[test]
fn array_copy_zero_extent_and_wholly_clipped_source_preserve_existing_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        destination(&mut context);
        clear(&mut context, [0.0, 0.0, 1.0, 1.0]);
        copy(&mut context, [0, 0, 4, 4]).unwrap();
        let budget = context.resource_bytes;
        for region in [[0, 0, 0, 4], [0, 0, 4, 0], [-10, -10, 4, 4], [10, 10, 4, 4]] {
            copy(&mut context, region).unwrap();
            assert_eq!(context.resource_bytes, budget);
        }
        let pixels = sample(&mut context, "vec3(0.5,0.5,1)");
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 0, 255, 255])
        );
    });
}
