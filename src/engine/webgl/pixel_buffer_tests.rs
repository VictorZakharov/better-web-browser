//! Author PBO bindings cannot influence private drawing-buffer storage or capture.
use super::tests::command;
use super::*;

#[test]
fn webgl2_private_surface_snapshot_and_resize_preserve_author_pixel_buffers() {
    let mut contexts = Contexts::default();
    let id = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
    let mut buffers = Vec::new();
    for target in [core_buffers::PIXEL_PACK, core_buffers::PIXEL_UNPACK] {
        let buffer = command(&mut contexts, id, "createBuffer", &[], &[], "", None)
            .as_u64()
            .unwrap() as u32;
        command(
            &mut contexts,
            id,
            "bindBuffer",
            &[target, buffer],
            &[],
            "",
            None,
        );
        command(
            &mut contexts,
            id,
            "bufferData",
            &[target, 4, gl::STATIC_DRAW],
            &[],
            "",
            Some(&[9, 8, 7, 6]),
        );
        buffers.push(buffer);
    }
    command(
        &mut contexts,
        id,
        "clearColor",
        &[],
        &[0., 1., 0., 1.],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "clear",
        &[gl::COLOR_BUFFER_BIT],
        &[],
        "",
        None,
    );
    let (_, _, pixels) = contexts
        .snapshot(id)
        .expect("private capture temporarily unbinds pack buffer");
    assert!(
        pixels
            .chunks_exact(4)
            .all(|pixel| pixel == [0, 255, 0, 255])
    );
    command(&mut contexts, id, "resize", &[4, 4], &[], "", None);
    let (_, _, pixels) = contexts
        .snapshot(id)
        .expect("private allocation temporarily unbinds unpack buffer");
    assert_eq!(pixels.len(), 64);
    assert!(pixels.iter().all(|byte| *byte == 0));
    for (target, pname, buffer) in [
        (core_buffers::PIXEL_PACK, 0x88ed, buffers[0]),
        (core_buffers::PIXEL_UNPACK, 0x88ef, buffers[1]),
    ] {
        assert_eq!(
            command(&mut contexts, id, "getParameter", &[pname], &[], "", None),
            json!(buffer)
        );
        let encoded = json!({"op":"getBufferSubData","i":[target,0,4]}).to_string();
        let PixelReply::Bytes(bytes) = contexts.read_pixels(id, &encoded, None) else {
            panic!("native PBO contents must remain readable");
        };
        assert_eq!(bytes, [9, 8, 7, 6]);
    }
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(0)
    );
}

#[test]
fn webgl2_owned_cpu_pixel_overloads_do_not_accept_bound_pixel_buffers() {
    let mut contexts = Contexts::default();
    let id = contexts.create(2, 2, r#"{"api":"webgl2"}"#).unwrap();
    let buffer = command(&mut contexts, id, "createBuffer", &[], &[], "", None)
        .as_u64()
        .unwrap() as u32;
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::PIXEL_PACK, buffer],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "bufferData",
        &[core_buffers::PIXEL_PACK, 4, gl::STATIC_DRAW],
        &[],
        "",
        Some(&[11, 12, 13, 14]),
    );
    let read = json!({"op":"readPixels","i":[0,0,1,1,gl::RGBA,gl::UNSIGNED_BYTE,4]}).to_string();
    assert!(matches!(
        contexts.read_pixels(id, &read, Some(&[1, 2, 3, 4])),
        PixelReply::Error
    ));
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::PIXEL_UNPACK, buffer],
        &[],
        "",
        None,
    );
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
        "texImage2D",
        &[
            gl::TEXTURE_2D,
            0,
            gl::RGBA,
            1,
            1,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
        ],
        &[],
        "",
        Some(&[1, 2, 3, 4]),
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(gl::INVALID_OPERATION)
    );
    let encoded = json!({"op":"getBufferSubData","i":[core_buffers::PIXEL_PACK,0,4]}).to_string();
    let PixelReply::Bytes(bytes) = contexts.read_pixels(id, &encoded, None) else {
        panic!("failed client-memory calls must preserve PBO storage");
    };
    assert_eq!(bytes, [11, 12, 13, 14]);
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::PIXEL_PACK, 0],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "bindBuffer",
        &[core_buffers::PIXEL_UNPACK, 0],
        &[],
        "",
        None,
    );
    command(
        &mut contexts,
        id,
        "texImage2D",
        &[
            gl::TEXTURE_2D,
            0,
            gl::RGBA,
            1,
            1,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
        ],
        &[],
        "",
        Some(&[1, 2, 3, 4]),
    );
    assert_eq!(
        command(&mut contexts, id, "getError", &[], &[], "", None),
        json!(0)
    );
    assert!(matches!(
        contexts.read_pixels(id, &read, Some(&[1, 2, 3, 4])),
        PixelReply::Bytes(_)
    ));
}
