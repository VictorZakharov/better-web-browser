//! Encoded blocks remain GPU-backed when transferred from pixel unpack buffers.
use super::api_version_tests::{call, version_two};
use super::compressed_capabilities::Family;
use super::compressed_core_tests::sample;
use super::compressed_texture_tests::{block, enable, texture, upload};
use super::*;
const UNPACK: i64 = 0x88ec;
fn buffer(context: &mut WebGl, bytes: &[u8]) -> i64 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as i64;
    call(context, "bindBuffer", &[UNPACK, id], "");
    upload(
        context,
        "bufferData",
        &[UNPACK, bytes.len() as i64, gl::STATIC_DRAW as i64],
        Some(bytes),
    )
    .unwrap();
    id
}

#[test]
fn compressed_buffer_offset_uploads_and_updates_read_exact_native_ranges() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        texture(&mut context, gl::TEXTURE_2D);
        let red = block(0x83f1, 0xf800);
        let green = block(0x83f1, 0x07e0);
        let bytes = [vec![91; 3], red.clone(), green.clone(), vec![92; 5]].concat();
        let id = buffer(&mut context, &bytes);
        upload(
            &mut context,
            "compressedTexImage2DFromBuffer",
            &[gl::TEXTURE_2D as i64, 0, 0x83f1, 8, 4, 0, 16, 3],
            None,
        )
        .unwrap();
        // Capture must temporarily unbind the unpack buffer and restore it.
        for (x, expected) in [("0.25", [255, 0, 0, 255]), ("0.75", [0, 255, 0, 255])] {
            let pixels = sample(
                &mut context,
                &format!("texture(t,vec2({x},0.5))"),
                "uniform sampler2D t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
            assert_eq!(call(&mut context, "getParameter", &[0x88ef], ""), json!(id));
        }
        upload(
            &mut context,
            "compressedTexSubImage2DFromBuffer",
            &[gl::TEXTURE_2D as i64, 0, 0, 0, 4, 4, 0x83f1, 8, 11],
            None,
        )
        .unwrap();
        let pixels = sample(
            &mut context,
            "texture(t,vec2(0.25,0.5))",
            "uniform sampler2D t;",
        );
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
    });
}

#[test]
fn compressed_buffer_invalid_ranges_sizes_and_pointer_overloads_are_atomic() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        let id = texture(&mut context, gl::TEXTURE_2D);
        let bytes = block(0x83f1, 0xf800);
        buffer(&mut context, &bytes);
        let args = [gl::TEXTURE_2D as i64, 0, 0x83f1, 4, 4, 0, 8, 0];
        for (index, value, error) in [
            (6, -1, gl::INVALID_VALUE),
            (6, 7, gl::INVALID_VALUE),
            (7, -1, gl::INVALID_VALUE),
            (7, 1, gl::INVALID_OPERATION),
            (7, i64::MAX, gl::INVALID_OPERATION),
        ] {
            let mut invalid = args;
            invalid[index] = value;
            assert_eq!(
                upload(
                    &mut context,
                    "compressedTexImage2DFromBuffer",
                    &invalid,
                    None
                ),
                Err(error)
            );
        }
        assert!(
            context
                .objects
                .get(id, Kind::Texture)
                .unwrap()
                .core_images
                .is_empty()
        );
        assert_eq!(
            upload(
                &mut context,
                "compressedTexImage2DFromBuffer",
                &args,
                Some(&bytes)
            ),
            Err(gl::INVALID_OPERATION)
        );
        assert_eq!(
            upload(
                &mut context,
                "compressedTexImage2D",
                &args[..6],
                Some(&bytes)
            ),
            Err(gl::INVALID_OPERATION)
        );
        upload(&mut context, "compressedTexImage2DFromBuffer", &args, None).unwrap();
        call(&mut context, "bindBuffer", &[UNPACK, 0], "");
        assert_eq!(
            upload(&mut context, "compressedTexImage2DFromBuffer", &args, None),
            Err(gl::INVALID_OPERATION)
        );
        let pixels = sample(&mut context, "texture(t,vec2(0.5))", "uniform sampler2D t;");
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [255, 0, 0, 255])
        );
    });
}

#[test]
fn compressed_array_buffer_offsets_select_real_encoded_slices() {
    session::run_native_test(|| {
        let mut context = version_two();
        enable(&mut context, Family::S3tc);
        texture(&mut context, 0x8c1a);
        let red = block(0x83f1, 0xf800);
        let green = block(0x83f1, 0x07e0);
        buffer(
            &mut context,
            &[vec![91; 3], red.clone(), green.clone()].concat(),
        );
        upload(
            &mut context,
            "compressedTexImage3DFromBuffer",
            &[0x8c1a, 0, 0x83f1, 4, 4, 2, 0, 16, 3],
            None,
        )
        .unwrap();
        for (layer, expected) in [(0, [255, 0, 0, 255]), (1, [0, 255, 0, 255])] {
            let pixels = sample(
                &mut context,
                &format!("texture(t,vec3(0.5,0.5,float({layer})))"),
                "uniform highp sampler2DArray t;",
            );
            assert!(pixels.chunks_exact(4).all(|pixel| pixel == expected));
        }
        upload(
            &mut context,
            "compressedTexSubImage3DFromBuffer",
            &[0x8c1a, 0, 0, 0, 0, 4, 4, 1, 0x83f1, 8, 11],
            None,
        )
        .unwrap();
        let pixels = sample(
            &mut context,
            "texture(t,vec3(0.5,0.5,0))",
            "uniform highp sampler2DArray t;",
        );
        assert!(
            pixels
                .chunks_exact(4)
                .all(|pixel| pixel == [0, 255, 0, 255])
        );
    });
}
