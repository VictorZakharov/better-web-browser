//! Byte-exact transfers exercise ANGLE, not only browser-side mirrors.
use super::tests::command;
use super::*;

fn context() -> (Contexts, u32) {
    let mut contexts = Contexts::default();
    let id = contexts
        .create(8, 4, r#"{"api":"webgl2"}"#)
        .expect("GLES3 native buffer provider");
    (contexts, id)
}

fn buffer(contexts: &mut Contexts, context: u32, target: u32, bytes: &[u8]) -> u32 {
    let id = command(contexts, context, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    command(
        contexts,
        context,
        "bindBuffer",
        &[target, id],
        &[],
        "",
        None,
    );
    command(
        contexts,
        context,
        "bufferData",
        &[target, bytes.len() as u32, gl::DYNAMIC_DRAW],
        &[],
        "",
        Some(bytes),
    );
    assert_eq!(
        command(contexts, context, "getError", &[], &[], "", None),
        json!(0)
    );
    id
}

fn read(contexts: &mut Contexts, context: u32, target: u32, offset: i64, size: i64) -> Vec<u8> {
    let encoded = json!({"op":"getBufferSubData","i":[target,offset,size]}).to_string();
    match contexts.read_pixels(context, &encoded, None) {
        PixelReply::Bytes(bytes) => bytes,
        PixelReply::Error => panic!(
            "readback error: {}",
            command(contexts, context, "getError", &[], &[], "", None)
        ),
        PixelReply::Lost => panic!("native context lost"),
    }
}

#[test]
fn webgl2_core_buffer_copy_and_native_mapping_preserve_exact_subranges() {
    let (mut contexts, id) = context();
    let source = buffer(
        &mut contexts,
        id,
        core_buffers::COPY_READ,
        &[0, 1, 2, 3, 4, 5, 6, 7],
    );
    let destination = buffer(&mut contexts, id, core_buffers::COPY_WRITE, &[99; 10]);
    command(
        &mut contexts,
        id,
        "copyBufferSubData",
        &[core_buffers::COPY_READ, core_buffers::COPY_WRITE, 2, 3, 4],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(0)
    );
    assert_eq!(
        read(&mut contexts, id, core_buffers::COPY_WRITE, 0, 10),
        [99, 99, 99, 2, 3, 4, 5, 99, 99, 99]
    );
    assert_eq!(
        read(&mut contexts, id, core_buffers::COPY_READ, 1, 5),
        [1, 2, 3, 4, 5]
    );
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, destination],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "bufferSubData",
        &[gl::ARRAY_BUFFER, 4],
        &[],
        "",
        Some(&[201, 202]),
    );
    assert_eq!(
        read(&mut contexts, id, core_buffers::COPY_WRITE, 3, 4),
        [2, 201, 202, 5]
    );
    assert!(read(&mut contexts, id, core_buffers::COPY_READ, 8, 0).is_empty());
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, source],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(0)
    );
}

#[test]
fn webgl2_core_buffer_overlapping_copy_and_out_of_range_reads_are_atomic() {
    let (mut contexts, id) = context();
    let buffer = buffer(
        &mut contexts,
        id,
        core_buffers::COPY_READ,
        &[1, 2, 3, 4, 5, 6, 7, 8],
    );
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::COPY_WRITE, buffer],
        &[],
        "",
        None,
    );
    for ranges in [[0, 2, 4], [0, 6, 3], [7, 0, 2]] {
        command(
            &mut contexts,
            id,
            "copyBufferSubData",
            &[
                core_buffers::COPY_READ,
                core_buffers::COPY_WRITE,
                ranges[0],
                ranges[1],
                ranges[2],
            ],
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_VALUE)
        );
        assert_eq!(
            read(&mut contexts, id, core_buffers::COPY_WRITE, 0, 8),
            [1, 2, 3, 4, 5, 6, 7, 8]
        );
    }
    command(
        &mut contexts,
        id,
        "copyBufferSubData",
        &[core_buffers::COPY_READ, core_buffers::COPY_WRITE, 0, 4, 4],
        &[],
        "",
        None,
    );
    assert_eq!(
        read(&mut contexts, id, core_buffers::COPY_WRITE, 0, 8),
        [1, 2, 3, 4, 1, 2, 3, 4]
    );
    for (offset, size) in [(-1, 1), (0, -1), (7, 2), (9, 0), (i64::MAX, 1)] {
        let encoded =
            json!({"op":"getBufferSubData","i":[core_buffers::COPY_WRITE,offset,size]}).to_string();
        assert!(matches!(
            contexts.read_pixels(id, &encoded, None),
            PixelReply::Error
        ));
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_VALUE)
        );
    }
}

#[test]
fn webgl2_core_buffer_neutral_targets_preserve_element_classification() {
    let (mut contexts, id) = context();
    let index = buffer(&mut contexts, id, gl::ELEMENT_ARRAY_BUFFER, &[1, 2, 3, 4]);
    let vertex = buffer(&mut contexts, id, gl::ARRAY_BUFFER, &[5, 6, 7, 8]);
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::COPY_READ, index],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::COPY_WRITE, vertex],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "copyBufferSubData",
        &[core_buffers::COPY_READ, core_buffers::COPY_WRITE, 0, 0, 4],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    assert_eq!(
        read(&mut contexts, id, core_buffers::COPY_WRITE, 0, 4),
        [5, 6, 7, 8]
    );
    for target in [
        core_buffers::UNIFORM,
        core_buffers::PIXEL_PACK,
        core_buffers::PIXEL_UNPACK,
    ] {
        command(
            &mut contexts,
            id,
            "bindBuffer",
            &[target, index],
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_OPERATION)
        );
        command(
            &mut contexts,
            id,
            "bindBuffer",
            &[target, vertex],
            &[],
            "",
            None,
        );
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(0)
        );
    }
}

#[test]
fn webgl2_core_buffer_deleted_bindings_and_cross_context_names_are_rejected() {
    let (mut contexts, id) = context();
    let buffer = buffer(&mut contexts, id, core_buffers::COPY_READ, &[1; 4]);
    let peer = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
    command(
        &mut contexts,
        peer,
        "bindBuffer",
        &[core_buffers::COPY_READ, buffer],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, peer, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    command(&mut contexts, id, "deleteBuffer", &[buffer], &[], "", None);
    let encoded = json!({"op":"getBufferSubData","i":[core_buffers::COPY_READ,0,4]}).to_string();
    assert!(matches!(
        contexts.read_pixels(id, &encoded, None),
        PixelReply::Error
    ));
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
}

#[test]
fn webgl1_does_not_admit_gles3_buffer_targets_or_commands() {
    let (mut contexts, id) = tests::context();
    for target in [
        core_buffers::COPY_READ,
        core_buffers::COPY_WRITE,
        core_buffers::UNIFORM,
        core_buffers::PIXEL_PACK,
        core_buffers::PIXEL_UNPACK,
        core_buffers::TRANSFORM_FEEDBACK,
    ] {
        command(&mut contexts, id, "bindBuffer", &[target, 0], &[], "", None);
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_ENUM)
        );
    }
    command(
        &mut contexts,
        id,
        "copyBufferSubData",
        &[core_buffers::COPY_READ, core_buffers::COPY_WRITE, 0, 0, 0],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
}
