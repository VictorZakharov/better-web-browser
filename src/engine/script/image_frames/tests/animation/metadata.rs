use super::super::*;
use crate::engine::script::image_frames::test_fixtures::{jpeg_segment, oriented_jpeg};

#[test]
fn exif_orientation_is_retained_as_frame_metadata_without_rotating_coded_pixels() {
    let expected = [
        (0, false),
        (0, true),
        (180, false),
        (180, true),
        (90, true),
        (90, false),
        (270, true),
        (270, false),
    ];
    let first = codecs::decode(
        "image/jpeg",
        &oriented_jpeg(1),
        true,
        &AtomicBool::new(false),
    )
    .unwrap();
    for (index, transform) in expected.into_iter().enumerate() {
        let result = codecs::decode(
            "image/jpeg",
            &oriented_jpeg(index as u16 + 1),
            true,
            &AtomicBool::new(false),
        )
        .unwrap();
        let frame = &result.frames[0];
        assert_eq!((frame.width, frame.height), (2, 1));
        assert_eq!((frame.rotation, frame.flip), transform);
        assert_eq!(
            frame.pixels, first.frames[0].pixels,
            "copyTo's coded sample order is unchanged"
        );
    }
}

#[test]
fn color_profile_choice_changes_pixels_without_erasing_orientation() {
    let jpeg = oriented_jpeg(6);
    let profile = moxcms::ColorProfile::new_display_p3().encode().unwrap();
    let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
    payload.extend(profile);
    let bytes = jpeg_segment(&jpeg, 0xe2, &payload);
    let converted = codecs::decode("image/jpeg", &bytes, false, &AtomicBool::new(false)).unwrap();
    let raw = codecs::decode("image/jpeg", &bytes, true, &AtomicBool::new(false)).unwrap();
    assert_ne!(
        converted.frames[0].pixels, raw.frames[0].pixels,
        "ICC conversion actually executes"
    );
    assert_eq!(converted.frames[0].rotation, 90);
    assert_eq!(raw.frames[0].rotation, 90);
    assert_eq!(
        (converted.frames[0].width, converted.frames[0].height),
        (2, 1)
    );
}

#[test]
fn malformed_icc_rejects_default_conversion_but_none_keeps_encoded_pixels() {
    let mut payload = b"ICC_PROFILE\0\x01\x01".to_vec();
    payload.extend([0; 32]);
    let bytes = jpeg_segment(&oriented_jpeg(1), 0xe2, &payload);
    assert!(codecs::decode("image/jpeg", &bytes, false, &AtomicBool::new(false)).is_err());
    assert!(codecs::decode("image/jpeg", &bytes, true, &AtomicBool::new(false)).is_ok());
}
