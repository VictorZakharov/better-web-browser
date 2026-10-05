//! Real 16-bit normalization, extension isolation, and allocation contracts.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::{texture, upload};
use super::normalized_textures::{FORMATS, RENDERABLE};
use super::*;

pub(super) fn enable(context: &mut WebGl) {
    assert_eq!(
        call(context, "enableExtension", &[], "EXT_texture_norm16"),
        json!(true)
    );
}

pub(super) fn words(values: &[u16]) -> Vec<u8> {
    values
        .iter()
        .flat_map(|value| value.to_ne_bytes())
        .collect()
}

pub(super) fn image(context: &mut WebGl, internal: u32, data: Option<&[u8]>) -> u32 {
    let id = texture(context, gl::TEXTURE_2D);
    let format = core_texture_formats::storage(internal).unwrap();
    upload(
        context,
        "texImage2D",
        &[
            gl::TEXTURE_2D as i64,
            0,
            internal as i64,
            1,
            1,
            0,
            format.base as i64,
            format.types[0] as i64,
        ],
        data,
    )
    .unwrap();
    id
}

pub(super) fn attach(context: &mut WebGl, texture: u32) -> u32 {
    let fb = call(context, "createFramebuffer", &[], "")
        .as_u64()
        .unwrap() as u32;
    call(
        context,
        "bindFramebuffer",
        &[gl::FRAMEBUFFER as i64, fb as i64],
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
    fb
}

#[test]
fn norm16_admission_is_webgl2_only_explicit_and_per_context() {
    session::run_native_test(|| {
        let mut first = version_two();
        let names = call(&mut first, "supportedExtensions", &[], "");
        assert!(
            names
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "EXT_texture_norm16")
        );
        texture(&mut first, gl::TEXTURE_2D);
        let bytes = first.resource_bytes;
        for internal in FORMATS {
            let format = core_texture_formats::storage(internal).unwrap();
            assert_eq!(
                upload(
                    &mut first,
                    "texStorage2D",
                    &[gl::TEXTURE_2D as i64, 1, internal as i64, 1, 1,],
                    None
                ),
                Err(gl::INVALID_ENUM)
            );
            assert_eq!(
                upload(
                    &mut first,
                    "texImage2D",
                    &[
                        gl::TEXTURE_2D as i64,
                        0,
                        internal as i64,
                        1,
                        1,
                        0,
                        format.base as i64,
                        format.types[0] as i64,
                    ],
                    None
                ),
                Err(gl::INVALID_VALUE)
            );
            assert_eq!(first.resource_bytes, bytes);
        }
        enable(&mut first);
        let mut peer = version_two();
        assert!(
            !peer
                .extensions
                .textures
                .enabled(texture_capabilities::TextureCapability::Norm16)
        );
        texture(&mut peer, gl::TEXTURE_2D);
        assert_eq!(
            upload(
                &mut peer,
                "texStorage2D",
                &[gl::TEXTURE_2D as i64, 1, 0x805b, 1, 1,],
                None
            ),
            Err(gl::INVALID_ENUM)
        );
        let mut legacy = WebGl::new(1, 1, Options::default()).unwrap();
        assert_eq!(
            call(&mut legacy, "enableExtension", &[], "EXT_texture_norm16"),
            json!(false)
        );
        assert!(
            !call(&mut legacy, "supportedExtensions", &[], "")
                .as_array()
                .unwrap()
                .iter()
                .any(|name| name == "EXT_texture_norm16")
        );
    });
}

#[test]
fn norm16_all_formats_allocate_owned_mutable_and_immutable_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        call(
            &mut context,
            "pixelStorei",
            &[gl::UNPACK_ALIGNMENT as i64, 1],
            "",
        );
        for internal in FORMATS {
            let format = core_texture_formats::storage(internal).unwrap();
            let bpp = core_texture_formats::upload(internal, format.base, format.types[0])
                .unwrap()
                .0;
            for target in [
                gl::TEXTURE_2D,
                texture_targets::VOLUME,
                texture_targets::ARRAY,
            ] {
                let id = texture(&mut context, target);
                let before = context.resource_bytes;
                let volume = target != gl::TEXTURE_2D;
                let mut args = vec![target as i64, 0, internal as i64, 2, 2];
                if volume {
                    args.push(2);
                }
                args.extend([0, format.base as i64, format.types[0] as i64]);
                let pixels = if volume { 8 } else { 4 };
                upload(
                    &mut context,
                    if volume { "texImage3D" } else { "texImage2D" },
                    &args,
                    Some(&vec![0; pixels * bpp]),
                )
                .unwrap();
                // The shared reservation charges two expanded native images.
                assert_eq!(context.resource_bytes - before, pixels * 16);
                assert_eq!(
                    context.objects.get(id, Kind::Texture).unwrap().core_images[&(target, 0)]
                        .internal,
                    internal
                );
                call(&mut context, "deleteTexture", &[id as i64], "");
                texture(&mut context, target);
                let mut immutable = vec![target as i64, 2, internal as i64, 2, 2];
                if volume {
                    immutable.push(2);
                }
                call(
                    &mut context,
                    if volume {
                        "texStorage3D"
                    } else {
                        "texStorage2D"
                    },
                    &immutable,
                    "",
                );
            }
        }
        assert_eq!(call(&mut context, "getError", &[], ""), json!(0));
    });
}

#[test]
fn norm16_unsigned_reads_preserve_precision_and_signed_formats_do_not_render() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        for internal in FORMATS {
            let format = core_texture_formats::storage(internal).unwrap();
            let channels = core_texture_formats::upload(internal, format.base, format.types[0])
                .unwrap()
                .0
                / 2;
            let values = [1, 32769, 65534, 65535];
            let id = image(&mut context, internal, Some(&words(&values[..channels])));
            attach(&mut context, id);
            let status = call(
                &mut context,
                "checkFramebufferStatus",
                &[gl::FRAMEBUFFER as i64],
                "",
            );
            if !RENDERABLE.contains(&internal) {
                assert_ne!(status, json!(gl::FRAMEBUFFER_COMPLETE));
                continue;
            }
            assert_eq!(status, json!(gl::FRAMEBUFFER_COMPLETE));
            let command = Command {
                op: "readPixels".into(),
                i: vec![0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_SHORT as i64, 8],
                f: vec![],
                text: String::new(),
            };
            let pixels = context.read_pixels(&command, None).unwrap();
            let actual: Vec<u16> = pixels
                .chunks_exact(2)
                .map(|word| u16::from_ne_bytes(word.try_into().unwrap()))
                .collect();
            let mut expected = vec![0, 0, 0, 65535];
            expected[..channels].copy_from_slice(&values[..channels]);
            assert_eq!(actual, expected, "{internal:#x}");
            assert_eq!(context.core_read_pair(gl::RGBA, gl::UNSIGNED_BYTE), Ok(4));
        }
    });
}

#[test]
fn norm16_wrong_format_and_type_cannot_replace_an_existing_image() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context);
        for internal in FORMATS {
            let id = image(&mut context, internal, None);
            let format = core_texture_formats::storage(internal).unwrap();
            let before = context.resource_bytes;
            for (base, kind) in [
                (format.base, gl::UNSIGNED_BYTE),
                (core_texture_formats::RGBA_INTEGER, format.types[0]),
            ] {
                assert_eq!(
                    upload(
                        &mut context,
                        "texImage2D",
                        &[
                            gl::TEXTURE_2D as i64,
                            0,
                            internal as i64,
                            2,
                            2,
                            0,
                            base as i64,
                            kind as i64,
                        ],
                        Some(&[0; 64])
                    ),
                    Err(gl::INVALID_OPERATION)
                );
                assert_eq!(context.resource_bytes, before);
                assert_eq!(
                    context.objects.get(id, Kind::Texture).unwrap().core_images
                        [&(gl::TEXTURE_2D, 0)]
                        .width,
                    1
                );
            }
        }
    });
}
