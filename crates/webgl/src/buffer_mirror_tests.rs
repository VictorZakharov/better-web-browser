//! CPU readback reuse must never conceal writes performed by the real GPU.
use super::api_version_tests::{call, version_two};
use super::compressed_texture_tests::upload;
use super::transform_feedback_tests::{INTERLEAVED, TARGET, assert_float_capture, shader};
use super::*;

fn buffer(context: &mut WebGl, target: u32, bytes: &[u8]) -> u32 {
    let id = call(context, "createBuffer", &[], "").as_u64().unwrap() as u32;
    call(context, "bindBuffer", &[target as i64, id as i64], "");
    upload(
        context,
        "bufferData",
        &[target as i64, bytes.len() as i64, gl::STATIC_DRAW as i64],
        Some(bytes),
    )
    .unwrap();
    id
}

fn read(context: &mut WebGl, target: u32, offset: usize, size: usize) -> Vec<u8> {
    context
        .read_buffer(&Command {
            op: "getBufferSubData".into(),
            i: vec![target as i64, offset as i64, size as i64],
            f: vec![],
            text: String::new(),
        })
        .unwrap()
}

#[test]
fn cpu_mirror_matches_native_mapping_after_upload_subdata_and_copies() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = buffer(
            &mut context,
            core_buffers::COPY_READ,
            &[1, 2, 3, 4, 5, 6, 7, 8],
        );
        let destination = buffer(&mut context, core_buffers::COPY_WRITE, &[99; 12]);
        call(
            &mut context,
            "copyBufferSubData",
            &[
                core_buffers::COPY_READ as i64,
                core_buffers::COPY_WRITE as i64,
                2,
                3,
                4,
            ],
            "",
        );
        upload(
            &mut context,
            "bufferSubData",
            &[core_buffers::COPY_WRITE as i64, 5],
            Some(&[31, 32]),
        )
        .unwrap();
        let expected = [99, 99, 99, 3, 4, 31, 32, 99, 99, 99, 99, 99];
        let charged = context.resource_bytes;
        for _ in 0..256 {
            assert_eq!(
                read(&mut context, core_buffers::COPY_WRITE, 0, 12),
                expected
            );
            assert!(
                context
                    .objects
                    .get(destination, Kind::Buffer)
                    .unwrap()
                    .buffer_mirror_valid
            );
        }
        // Force the actual map path and compare it with the authoritative CPU
        // result. This is a test-only invalidation, never a production shortcut.
        context
            .objects
            .get_mut(destination, Kind::Buffer)
            .unwrap()
            .buffer_mirror_valid = false;
        assert_eq!(
            read(&mut context, core_buffers::COPY_WRITE, 2, 6),
            expected[2..8]
        );
        assert!(
            !context
                .objects
                .get(destination, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(
            read(&mut context, core_buffers::COPY_WRITE, 0, 12),
            expected
        );
        assert!(
            context
                .objects
                .get(destination, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert!(
            context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(
            context.resource_bytes, charged,
            "reuse allocates no retained cache"
        );
    });
}

#[test]
fn pixel_pack_and_dirty_buffer_copies_never_return_stale_cpu_bytes() {
    session::run_native_test(|| {
        let mut context = version_two();
        let source = buffer(&mut context, core_buffers::PIXEL_PACK, &[19; 8]);
        context
            .dispatch(
                &Command {
                    op: "clearColor".into(),
                    i: vec![],
                    f: vec![0., 1., 0., 1.],
                    text: String::new(),
                },
                None,
            )
            .unwrap();
        call(&mut context, "clear", &[gl::COLOR_BUFFER_BIT as i64], "");
        call(
            &mut context,
            "readPixelsToBuffer",
            &[0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 0],
            "",
        );
        assert!(
            !context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(
            read(&mut context, core_buffers::PIXEL_PACK, 0, 4),
            [0, 255, 0, 255]
        );
        // A partial CPU overwrite must preserve the GPU-written first pixel.
        upload(
            &mut context,
            "bufferSubData",
            &[core_buffers::PIXEL_PACK as i64, 4],
            Some(&[21, 22, 23, 24]),
        )
        .unwrap();
        assert!(
            !context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        call(
            &mut context,
            "bindBuffer",
            &[core_buffers::COPY_READ as i64, source as i64],
            "",
        );
        let destination = buffer(&mut context, core_buffers::COPY_WRITE, &[31; 8]);
        call(
            &mut context,
            "copyBufferSubData",
            &[
                core_buffers::COPY_READ as i64,
                core_buffers::COPY_WRITE as i64,
                0,
                0,
                8,
            ],
            "",
        );
        assert!(
            !context
                .objects
                .get(destination, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(
            read(&mut context, core_buffers::COPY_WRITE, 0, 8),
            [0, 255, 0, 255, 21, 22, 23, 24]
        );
        assert!(
            context
                .objects
                .get(destination, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert!(
            !context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(
            read(&mut context, core_buffers::PIXEL_PACK, 0, 8),
            [0, 255, 0, 255, 21, 22, 23, 24]
        );
        assert!(
            context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        // A full CPU overwrite re-establishes authority without native mapping.
        upload(
            &mut context,
            "bufferSubData",
            &[core_buffers::PIXEL_PACK as i64, 0],
            Some(&[7; 8]),
        )
        .unwrap();
        assert!(
            context
                .objects
                .get(source, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        assert_eq!(read(&mut context, core_buffers::PIXEL_PACK, 0, 8), [7; 8]);
    });
}

#[test]
fn transform_capture_and_resumption_never_reuse_uncaptured_cpu_storage() {
    session::run_native_test(|| {
        let mut context = version_two();
        shader(&mut context, INTERLEAVED, r#"["captured"]"#);
        let id = buffer(&mut context, TARGET as u32, &[17; 16]);
        call(&mut context, "bindBufferBase", &[TARGET, 0, id as i64], "");
        call(&mut context, "enable", &[0x8c89], "");
        call(
            &mut context,
            "beginTransformFeedback",
            &[gl::POINTS as i64],
            "",
        );
        assert!(
            !context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        call(&mut context, "drawArrays", &[gl::POINTS as i64, 0, 1], "");
        call(&mut context, "pauseTransformFeedback", &[], "");
        assert!(
            !context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        upload(&mut context, "bufferSubData", &[TARGET, 0], Some(&[0; 16])).unwrap();
        assert!(
            context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        call(&mut context, "resumeTransformFeedback", &[], "");
        assert!(
            !context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
        call(&mut context, "drawArrays", &[gl::POINTS as i64, 1, 1], "");
        call(&mut context, "endTransformFeedback", &[], "");
        let bytes = read(&mut context, TARGET as u32, 0, 16);
        assert_float_capture(&bytes, &[0., 0., 1.5, -1.]);
        assert!(
            context
                .objects
                .get(id, Kind::Buffer)
                .unwrap()
                .buffer_mirror_valid
        );
    });
}
