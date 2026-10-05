//! Texture identity alone is not feedback: level/layer identify the image.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::texture_targets::VOLUME;
use super::*;

fn attach(context: &mut WebGl, texture: u32, level: i64, layer: i64) {
    call(
        context,
        "framebufferTextureLayer",
        &[
            gl::FRAMEBUFFER as i64,
            gl::COLOR_ATTACHMENT0 as i64,
            texture as i64,
            level,
            layer,
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
}

fn scene(context: &mut WebGl) -> u32 {
    let id = texture(context, VOLUME);
    let bytes = [[255, 0, 0, 255].repeat(4), [0, 0, 255, 255].repeat(4)].concat();
    upload(
        context,
        "texImage3D",
        &[
            VOLUME as i64,
            0,
            gl::RGBA as i64,
            2,
            2,
            2,
            0,
            gl::RGBA as i64,
            gl::UNSIGNED_BYTE as i64,
        ],
        Some(&bytes),
    )
    .unwrap();
    call(context, "generateMipmap", &[VOLUME as i64], "");
    let fb = call(context, "createFramebuffer", &[], "")
        .as_i64()
        .unwrap();
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, fb],
        "",
    );
    attach(context, id, 0, 0);
    id
}

fn copy(context: &mut WebGl, level: i64, layer: i64, size: i64) -> Result<Value> {
    upload(
        context,
        "copyTexSubImage3D",
        &[VOLUME as i64, level, 0, 0, layer, 0, 0, size, size],
        None,
    )
}

fn read(context: &mut WebGl, size: i64) -> Vec<u8> {
    context
        .read_pixels(
            &Command {
                op: "readPixels".into(),
                i: vec![
                    0,
                    0,
                    size,
                    size,
                    gl::RGBA as i64,
                    gl::UNSIGNED_BYTE as i64,
                    size * size * 4,
                ],
                f: vec![],
                text: String::new(),
            },
            None,
        )
        .unwrap()
}

#[test]
fn volume_copy_rejects_same_image_feedback_without_changing_texels() {
    session::run_native_test(|| {
        let mut context = version_two();
        scene(&mut context);
        assert_eq!(copy(&mut context, 0, 0, 2), Err(gl::INVALID_OPERATION));
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    });
}

#[test]
fn volume_copy_between_distinct_levels_of_one_texture_keeps_source_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = scene(&mut context);
        copy(&mut context, 1, 0, 1).unwrap();
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        attach(&mut context, id, 1, 0);
        assert_eq!(read(&mut context, 1), [255, 0, 0, 255]);
    });
}

#[test]
fn volume_copy_between_distinct_layers_of_one_texture_keeps_source_pixels() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = scene(&mut context);
        copy(&mut context, 0, 1, 2).unwrap();
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        attach(&mut context, id, 0, 1);
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
    });
}

#[test]
fn volume_copy_between_distinct_volume_objects_reads_the_selected_source_layer() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = scene(&mut context);
        let destination = texture(&mut context, VOLUME);
        call(
            &mut context,
            "texStorage3D",
            &[VOLUME as i64, 1, 0x8058, 2, 2, 2],
            "",
        );
        copy(&mut context, 0, 1, 2).unwrap();
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        attach(&mut context, destination, 0, 1);
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        attach(&mut context, destination, 0, 0);
        assert!(read(&mut context, 2).iter().all(|v| *v == 0));
        attach(&mut context, source, 0, 1);
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [0, 0, 255, 255])
        );
    });
}

#[test]
fn volume_staging_preserves_bindings_scissor_and_pixel_buffer_state() {
    session::run_native_test(|| {
        let mut context = version_two();
        let id = scene(&mut context);
        let author_texture = texture(&mut context, gl::TEXTURE_2D);
        let mut buffers = Vec::new();
        for target in [core_buffers::PIXEL_PACK, core_buffers::PIXEL_UNPACK] {
            let buffer = call(&mut context, "createBuffer", &[], "")
                .as_i64()
                .unwrap();
            call(&mut context, "bindBuffer", &[target as i64, buffer], "");
            call(
                &mut context,
                "bufferData",
                &[target as i64, 64, gl::STATIC_DRAW as i64],
                "",
            );
            buffers.push(buffer);
        }
        call(&mut context, "enable", &[gl::SCISSOR_TEST as i64], "");
        call(&mut context, "scissor", &[0, 0, 0, 0], "");
        call(
            &mut context,
            "pixelStorei",
            &[gl::UNPACK_ALIGNMENT as i64, 8],
            "",
        );
        let charged = context.resource_bytes;
        context.resource_limit = charged + 31;
        assert_eq!(copy(&mut context, 0, 1, 2), Err(gl::OUT_OF_MEMORY));
        context.resource_limit = charged + 32;
        copy(&mut context, 0, 1, 2).unwrap();
        assert_eq!(context.resource_bytes, charged);
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::TEXTURE_BINDING_2D as i64],
                ""
            ),
            json!(author_texture)
        );
        assert_eq!(
            call(
                &mut context,
                "getParameter",
                &[gl::UNPACK_ALIGNMENT as i64],
                ""
            ),
            json!(8)
        );
        assert_eq!(
            call(&mut context, "isEnabled", &[gl::SCISSOR_TEST as i64], ""),
            json!(true)
        );
        for (pname, buffer) in [0x88ed, 0x88ef].into_iter().zip(buffers) {
            assert_eq!(
                call(&mut context, "getParameter", &[pname], ""),
                json!(buffer)
            );
        }
        let mut native = 0;
        unsafe {
            gl::GetIntegerv(gl::TEXTURE_BINDING_2D, &mut native);
        }
        assert_eq!(
            native as u32,
            context.objects.name(author_texture, Kind::Texture).unwrap()
        );
        call(
            &mut context,
            "bindBuffer",
            &[core_buffers::PIXEL_PACK as i64, 0],
            "",
        );
        attach(&mut context, id, 0, 1);
        assert!(
            read(&mut context, 2)
                .chunks_exact(4)
                .all(|p| p == [255, 0, 0, 255])
        );
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}
