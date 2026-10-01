//! Malformed host requests must never index or allocate outside admitted data.
use super::*;
use crate::engine::script::JsValue;

fn call(jobs: &mut ImageFrames, operation: &str, args: Vec<JsValue>) -> Result<JsValue, String> {
    let mut arguments = vec![JsValue::from(operation.to_owned())];
    arguments.extend(args);
    super::super::host::dispatch(operation, &arguments, jobs)
        .map(|result| result.expect("known image operation"))
        .map_err(|error| error.to_string())
}

fn member<'a>(value: &'a JsValue, key: &str) -> &'a JsValue {
    let JsValue::Object(members) = value else {
        panic!("expected host record")
    };
    &members.iter().find(|(name, _)| name == key).unwrap().1
}

#[test]
fn protocol_frame_chunks_have_consistent_metadata_and_owned_bytes() {
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/png", &png(), false).unwrap();
    ready(&mut jobs, id);
    let value = call(
        &mut jobs,
        "imageDecoderFrame",
        vec![id.into(), 0.into(), 0.into(), 0.into()],
    )
    .unwrap();
    assert_eq!(member(&value, "width").as_number(), Some(2.0));
    assert_eq!(member(&value, "height").as_number(), Some(1.0));
    assert_eq!(member(&value, "timestamp").as_number(), Some(0.0));
    assert!(matches!(member(&value, "duration"), JsValue::Null));
    assert_eq!(member(&value, "done").as_boolean(), Some(true));
    assert_eq!(
        member(&value, "pixels").as_bytes(),
        Some([1, 2, 3, 255, 5, 6, 7, 128].as_slice())
    );
    jobs.close(id);
    assert_eq!(
        member(&value, "pixels").as_bytes().unwrap().len(),
        8,
        "returned bytes outlive session"
    );
}

#[test]
fn invalid_ids_offsets_indices_and_track_numbers_are_rejected() {
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/png", &png(), false).unwrap();
    ready(&mut jobs, id);
    for arguments in [
        vec![0.into(), 0.into(), 0.into(), 0.into()],
        vec![id.into(), 1.into(), 0.into(), 0.into()],
        vec![id.into(), 0.into(), 1.into(), 0.into()],
        vec![id.into(), 0.into(), (CHUNK_BYTES as u32).into(), 0.into()],
        vec![id.into(), 0.into(), 0.into(), 1.into()],
        vec![id.into(), 0.into(), 0.into()],
    ] {
        assert!(call(&mut jobs, "imageDecoderFrame", arguments).is_err());
    }
    for number in [f64::NAN, f64::INFINITY, -1.0, 0.5, u32::MAX as f64 + 1.0] {
        assert!(call(&mut jobs, "imageDecoderPoll", vec![number.into()]).is_err());
    }
    assert!(call(&mut jobs, "imageDecoderClose", vec![JsValue::Null]).is_err());
    assert_eq!(
        ready(&mut jobs, id).frames.len(),
        1,
        "bad requests do not corrupt valid job"
    );
}

#[test]
fn chunk_protocol_caps_each_response_and_reconstructs_the_full_image() {
    let width = 256;
    let height = 80;
    let pixels = (0..width * height * 4)
        .map(|index| (index % 251) as u8)
        .collect::<Vec<_>>();
    let image = image::RgbaImage::from_raw(width, height, pixels.clone()).unwrap();
    let mut encoded = Cursor::new(Vec::new());
    DynamicImage::ImageRgba8(image)
        .write_to(&mut encoded, ImageFormat::Png)
        .unwrap();
    let mut jobs = ImageFrames::default();
    let id = jobs.start("image/png", encoded.get_ref(), true).unwrap();
    ready(&mut jobs, id);
    let mut reconstructed = Vec::new();
    loop {
        let offset = reconstructed.len();
        let value = call(
            &mut jobs,
            "imageDecoderFrame",
            vec![id.into(), 0.into(), (offset as u32).into(), 0.into()],
        )
        .unwrap();
        let bytes = member(&value, "pixels").as_bytes().unwrap();
        assert!(!bytes.is_empty() && bytes.len() <= CHUNK_BYTES);
        assert_eq!(member(&value, "offset").as_number(), Some(offset as f64));
        reconstructed.extend_from_slice(bytes);
        if member(&value, "done").as_boolean() == Some(true) {
            break;
        }
        assert_eq!(bytes.len(), CHUNK_BYTES);
    }
    assert_eq!(reconstructed, pixels);
}

#[test]
fn poster_and_animation_are_separate_protocol_resources() {
    let mut jobs = ImageFrames::default();
    let id = jobs
        .start("image/png", &super::super::test_fixtures::apng(true), false)
        .unwrap();
    ready(&mut jobs, id);
    let status = call(&mut jobs, "imageDecoderPoll", vec![id.into()]).unwrap();
    assert_eq!(member(&status, "poster").as_boolean(), Some(true));
    assert_eq!(member(&status, "frameCount").as_number(), Some(3.0));
    let poster = call(
        &mut jobs,
        "imageDecoderFrame",
        vec![id.into(), 0.into(), 0.into(), 0.into()],
    )
    .unwrap();
    assert_eq!(
        &member(&poster, "pixels").as_bytes().unwrap()[..4],
        &[90, 80, 70, 255]
    );
    let animation = call(
        &mut jobs,
        "imageDecoderFrame",
        vec![id.into(), 2.into(), 0.into(), 1.into()],
    )
    .unwrap();
    assert_eq!(member(&animation, "timestamp").as_number(), Some(80000.0));
    assert!(
        call(
            &mut jobs,
            "imageDecoderFrame",
            vec![id.into(), 1.into(), 0.into(), 0.into()]
        )
        .is_err()
    );
}

#[test]
fn unknown_operations_leave_the_registry_untouched() {
    let mut jobs = ImageFrames::default();
    assert!(
        super::super::host::dispatch("notAnImageOperation", &[], &mut jobs)
            .unwrap()
            .is_none()
    );
    assert!(jobs.sessions.is_empty());
    assert!(
        call(
            &mut jobs,
            "imageDecoderStart",
            vec![JsValue::from("image/png".to_owned()), JsValue::Null]
        )
        .is_err()
    );
    assert!(jobs.sessions.is_empty());
}
