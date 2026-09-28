use super::*;
use std::io::Cursor;

fn session() -> CaptureSessionId {
    CaptureSessionId::new(9).unwrap()
}

fn sample(kind: CaptureSampleKind) -> CaptureSample {
    match kind {
        CaptureSampleKind::VideoNv12 => CaptureSample {
            capture_id: 3,
            track_id: 1,
            sequence: 4,
            timestamp_100ns: 123_000,
            kind,
            width_or_rate: 4,
            height_or_frames: 2,
            stride_or_channels: 4,
            bytes: vec![128; 12],
        },
        CaptureSampleKind::AudioPcm16 => CaptureSample {
            capture_id: 3,
            track_id: 2,
            sequence: 4,
            timestamp_100ns: 123_000,
            kind,
            width_or_rate: 48_000,
            height_or_frames: 960,
            stride_or_channels: 1,
            bytes: vec![0; 1_920],
        },
    }
}

#[test]
fn browser_control_is_sequenced_and_exactly_bounded() {
    let nonce = Nonce::new([5; 32]);
    let devices = CaptureDevices {
        camera: true,
        microphone: false,
    };
    let messages = [
        BrowserCaptureMessage::Hello { nonce, devices },
        BrowserCaptureMessage::Start { capture_id: 3 },
        BrowserCaptureMessage::Stop { capture_id: 3 },
        BrowserCaptureMessage::Shutdown,
    ];
    let mut bytes = Vec::new();
    {
        let mut writer = CaptureFrameWriter::new(&mut bytes, session());
        for message in &messages {
            writer.send_browser(message).unwrap();
        }
    }
    let mut reader = CaptureFrameReader::new(Cursor::new(bytes.clone()), session());
    for message in messages {
        assert_eq!(reader.read_browser().unwrap(), message);
    }
    bytes[24..32].copy_from_slice(&2_u64.to_le_bytes());
    let mut reader = CaptureFrameReader::new(Cursor::new(bytes), session());
    assert!(matches!(
        reader.read_browser(),
        Err(CaptureProtocolError::WrongSequence)
    ));
}

#[test]
fn worker_control_never_crosses_sample_pipe() {
    let mut bytes = Vec::new();
    CaptureFrameWriter::new(&mut bytes, session())
        .send_worker(&WorkerCaptureMessage::Ready {
            nonce: Nonce::new([7; 32]),
        })
        .unwrap();
    let mut reader = CaptureFrameReader::new(Cursor::new(bytes), session());
    assert!(matches!(
        reader.read_sample(),
        Err(CaptureProtocolError::UnexpectedMessage)
    ));
}

#[test]
fn video_and_audio_round_trip_without_format_ambiguity() {
    for kind in [CaptureSampleKind::VideoNv12, CaptureSampleKind::AudioPcm16] {
        let original = sample(kind);
        let mut bytes = Vec::new();
        CaptureFrameWriter::new(&mut bytes, session())
            .send_sample(&original)
            .unwrap();
        let restored = CaptureFrameReader::new(Cursor::new(bytes), session())
            .read_sample()
            .unwrap();
        assert_eq!(restored, original);
    }
}

#[test]
fn malformed_shapes_and_lengths_fail_before_allocation_or_delivery() {
    let mut frame = sample(CaptureSampleKind::VideoNv12);
    frame.stride_or_channels = 2;
    assert!(frame.validate().is_err());
    frame.stride_or_channels = 4;
    frame.bytes.pop();
    assert!(frame.validate().is_err());

    let mut frame = sample(CaptureSampleKind::AudioPcm16);
    frame.height_or_frames = 961;
    assert!(frame.validate().is_err());
    frame.height_or_frames = 960;
    frame.width_or_rate = 8_000;
    assert!(frame.validate().is_err());

    let mut encoded = Vec::new();
    CaptureFrameWriter::new(&mut encoded, session())
        .send_sample(&sample(CaptureSampleKind::VideoNv12))
        .unwrap();
    encoded[12..16].copy_from_slice(&((MAX_SAMPLE_BYTES + 50) as u32).to_le_bytes());
    let mut reader = CaptureFrameReader::new(Cursor::new(encoded), session());
    assert!(matches!(
        reader.read_sample(),
        Err(CaptureProtocolError::PayloadTooLarge)
    ));
}

#[test]
fn empty_grants_and_unknown_bits_are_rejected() {
    assert!(
        CaptureDevices {
            camera: false,
            microphone: false
        }
        .validate()
        .is_err()
    );
    assert!(CaptureDevices::from_bits(4).is_err());
    assert_eq!(CaptureDevices::from_bits(3).unwrap().bits(), 3);
}
