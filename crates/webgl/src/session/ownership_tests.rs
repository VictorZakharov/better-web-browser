//! Movable byte allocations still belong to exactly one realm's registry.
use super::*;

fn command(op: &str, integers: &[i64]) -> String {
    json!({"op":op,"i":integers}).to_string()
}

#[test]
fn owned_transfers_cannot_route_to_a_peer_realms_context() {
    let mut owner = Contexts::default();
    let mut peer = Contexts::default();
    let first = owner.create(1, 1, "{\"api\":\"webgl2\"}").unwrap();
    let second = peer.create(1, 1, "{\"api\":\"webgl2\"}").unwrap();
    assert_ne!(first, second);
    let buffer = owner
        .execute(first, &command("createBuffer", &[]), None)
        .as_u64()
        .unwrap();
    owner.execute(
        first,
        &command("bindBuffer", &[gl::ARRAY_BUFFER as i64, buffer as i64]),
        None,
    );
    let upload = command(
        "bufferData",
        &[gl::ARRAY_BUFFER as i64, 4, gl::STATIC_DRAW as i64],
    );
    assert_eq!(
        owner.execute_owned(first, &upload, Some(vec![1, 2, 3, 4])),
        Value::Null
    );
    assert_eq!(
        peer.execute_owned(first, &upload, Some(vec![5, 6, 7, 8])),
        json!({"lost":true})
    );
    let read = command("getBufferSubData", &[gl::ARRAY_BUFFER as i64, 0, 4]);
    assert!(matches!(
        peer.read_pixels_owned(first, &read, None),
        PixelReply::Lost
    ));
    let PixelReply::Bytes(bytes) = owner.read_pixels_owned(first, &read, None) else {
        panic!("owner's native buffer must remain readable");
    };
    assert_eq!(bytes, [1, 2, 3, 4]);
    assert_eq!(
        owner.execute(first, &command("getError", &[]), None),
        json!(0)
    );
}

#[test]
fn owned_reads_and_uploads_after_retirement_cannot_alias_a_restored_context() {
    let mut owner = Contexts::default();
    let original = owner.create(1, 1, "{}").unwrap();
    owner.remove(original);
    let restored = owner.create(1, 1, "{}").unwrap();
    assert_ne!(original, restored);
    let clear = r#"{"op":"clearColor","f":[0,1,0,1]}"#;
    owner.execute(restored, clear, None);
    owner.execute(
        restored,
        &command("clear", &[gl::COLOR_BUFFER_BIT as i64]),
        None,
    );
    let read = command(
        "readPixels",
        &[0, 0, 1, 1, gl::RGBA as i64, gl::UNSIGNED_BYTE as i64, 4],
    );
    assert!(matches!(
        owner.read_pixels_owned(original, &read, Some(vec![83; 4])),
        PixelReply::Lost
    ));
    assert_eq!(
        owner.execute_owned(
            original,
            &command(
                "bufferData",
                &[gl::ARRAY_BUFFER as i64, 4, gl::STATIC_DRAW as i64]
            ),
            Some(vec![83; 4])
        ),
        json!({"lost":true})
    );
    let PixelReply::Bytes(bytes) = owner.read_pixels_owned(restored, &read, Some(vec![83; 4]))
    else {
        panic!("restored context must still render its own pixels");
    };
    assert_eq!(bytes, [0, 255, 0, 255]);
    assert_eq!(
        owner.execute(restored, &command("getError", &[]), None),
        json!(0)
    );
}

#[test]
fn owned_upload_flushes_prior_binding_commands_before_native_submission() {
    let mut owner = Contexts::default();
    let id = owner.create(1, 1, "{\"api\":\"webgl2\"}").unwrap();
    let buffers: Vec<_> = (0..2)
        .map(|_| {
            owner
                .execute(id, &command("createBuffer", &[]), None)
                .as_u64()
                .unwrap()
        })
        .collect();
    for (index, buffer) in buffers.iter().enumerate() {
        owner.execute(
            id,
            &command("bindBuffer", &[gl::ARRAY_BUFFER as i64, *buffer as i64]),
            None,
        );
        owner.execute_owned(
            id,
            &command(
                "bufferData",
                &[gl::ARRAY_BUFFER as i64, 4, gl::STATIC_DRAW as i64],
            ),
            Some(vec![index as u8 + 1; 4]),
        );
    }
    for (index, buffer) in buffers.iter().enumerate() {
        owner.execute(
            id,
            &command("bindBuffer", &[gl::ARRAY_BUFFER as i64, *buffer as i64]),
            None,
        );
        let read = command("getBufferSubData", &[gl::ARRAY_BUFFER as i64, 0, 4]);
        let PixelReply::Bytes(bytes) = owner.read_pixels_owned(id, &read, None) else {
            panic!("prior binding must be visible to binary readback");
        };
        assert_eq!(bytes, [index as u8 + 1; 4]);
    }
    assert_eq!(owner.execute(id, &command("getError", &[]), None), json!(0));
}
