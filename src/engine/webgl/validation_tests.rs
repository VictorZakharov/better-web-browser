//! Boundary tests deliberately bypass JavaScript validation to exercise native safety.
use super::tests::{command, context};
use super::*;

#[test]
fn packed_pixel_lengths_cover_padding_zero_dimensions_and_overflow() {
    use super::textures::pixel_size;
    assert_eq!(pixel_size(1, 1, 3, 4), Ok(3));
    assert_eq!(pixel_size(1, 2, 3, 4), Ok(7));
    assert_eq!(pixel_size(3, 3, 3, 8), Ok(41));
    assert_eq!(pixel_size(0, usize::MAX, 4, 4), Ok(0));
    assert_eq!(pixel_size(usize::MAX, 0, 4, 4), Ok(0));
    assert_eq!(pixel_size(1, 1, 4, 3), Err(gl::INVALID_VALUE));
    assert_eq!(pixel_size(usize::MAX, 1, 4, 4), Err(gl::OUT_OF_MEMORY));
    assert_eq!(pixel_size(1, usize::MAX, 4, 8), Err(gl::OUT_OF_MEMORY));
}

#[test]
fn unrecognized_query_sizes_cannot_write_native_memory() {
    let (mut contexts, id) = context();
    for op in ["getParameter", "isEnabled", "getShaderPrecisionFormat"] {
        command(&mut contexts, id, op, &[u32::MAX, u32::MAX], &[], "", None);
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_ENUM)
        );
    }
    for text in [
        r#"{"op":"getParameter","i":[-1]}"#,
        r#"{"op":"getParameter","i":[4294967296]}"#,
        r#"{"op":"clearColor","f":["infinity",0,0,1]}"#,
        r#"{"op":"readPixels","i":[0,0,2147483648,1,6408,5121,16]}"#,
        r#"{"op":"clear","i":[0],"native_pointer":1234}"#,
    ] {
        contexts.execute(id, text, None);
        assert_eq!(
            command(&mut contexts, id, "getError", &[], &[], "", None),
            json!(gl::INVALID_VALUE)
        );
    }
    assert!(
        contexts
            .snapshot(id)
            .unwrap()
            .2
            .iter()
            .all(|value| *value == 0)
    );
}

#[test]
fn texture_alignment_and_target_binding_are_checked_before_upload() {
    let (mut contexts, id) = context();
    let texture = command(&mut contexts, id, "createTexture", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "bindTexture",
        &[gl::TEXTURE_2D, texture],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "bindTexture",
        &[gl::TEXTURE_CUBE_MAP, texture],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    // Two RGB rows at alignment four require seven bytes, not six or eight.
    command(
        &mut contexts,
        id,
        "texImage2D",
        &[
            gl::TEXTURE_2D,
            0,
            gl::RGB,
            1,
            2,
            0,
            gl::RGB,
            gl::UNSIGNED_BYTE,
        ],
        &[],
        "",
        Some(&[1, 2, 3, 4, 5, 6]),
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    command(
        &mut contexts,
        id,
        "texImage2D",
        &[
            gl::TEXTURE_2D,
            0,
            gl::RGB,
            1,
            2,
            0,
            gl::RGB,
            gl::UNSIGNED_BYTE,
        ],
        &[],
        "",
        Some(&[1, 2, 3, 0, 4, 5, 6]),
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::NO_ERROR)
    );
    command(
        &mut contexts,
        id,
        "texImage2D",
        &[
            gl::TEXTURE_2D,
            0,
            gl::RGBA,
            4097,
            1,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
        ],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_VALUE)
    );
}

#[test]
fn oversized_payload_rejection_does_not_copy_or_modify_driver_storage() {
    let (mut contexts, id) = context();
    let code = "x".repeat(MAX_SHADER_BYTES + 4097);
    assert_eq!(contexts.execute(id, &code, None), Value::Null);
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::OUT_OF_MEMORY)
    );
    let upload = vec![0u8; MAX_UPLOAD_BYTES + 1];
    contexts.execute(id, r#"{"op":"clear","i":[16384]}"#, Some(&upload));
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::OUT_OF_MEMORY)
    );
    assert!(
        contexts
            .snapshot(id)
            .unwrap()
            .2
            .iter()
            .all(|value| *value == 0)
    );
}

#[test]
fn removal_and_recreation_never_alias_context_or_resource_names() {
    let (mut contexts, id) = context();
    let buffer = command(&mut contexts, id, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    contexts.remove(id);
    assert!(contexts.snapshot(id).is_none());
    let next = contexts.create(16, 16, "{}").unwrap();
    assert_ne!(next, id);
    let next_buffer = command(&mut contexts, next, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    assert_ne!(next_buffer, buffer);
    command(
        &mut contexts,
        next,
        "bindBuffer",
        &[gl::ARRAY_BUFFER, buffer],
        &[],
        "",
        None,
    );
    assert_eq!(
        command(&mut contexts, next, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    contexts.clear();
    assert!(contexts.snapshot(next).is_none());
}
